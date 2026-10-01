//! Syslog over UDP, one message per configured device host.
//!
//! Message layout matches the Java edition (`syslog-java-client`):
//!
//! * `RFC_3164`: `<PRI>MMM dd HH:mm:ss HOST APP: MSG` (local time)
//! * `RFC_5424`: `<PRI>1 yyyy-MM-ddTHH:mm:ss.SSSZ HOST APP - - - MSG` (UTC)
//! * `RFC_5425`: RFC 5424 message prefixed with its octet length
//!
//! The server host name is resolved when sending (cached for a minute, IPv4
//! preferred), so a sender stays usable while DNS is temporarily unavailable.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{Local, Utc};
use logmaker_plugin_api::{
    ArgSpec, ArgType, Args, PluginError, Sender, SenderFactory, arg_i64, arg_str, arg_string_list,
};
use parking_lot::RwLock;

const FACILITY_USER: u8 = 1;
const SEVERITY_INFORMATIONAL: u8 = 6;
/// How long a resolved server address is reused.
const RESOLVE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageFormat {
    Rfc3164,
    Rfc5424,
    Rfc5425,
}

impl MessageFormat {
    /// Unknown names fall back to RFC 5424, like the Java edition.
    fn from_name(name: &str) -> Self {
        match name {
            "RFC_3164" => Self::Rfc3164,
            "RFC_5425" => Self::Rfc5425,
            _ => Self::Rfc5424,
        }
    }
}

pub struct SyslogFactory;

/// Where messages go: a literal address, or a host name resolved on demand.
enum Target {
    Fixed(SocketAddr),
    Host {
        host: String,
        port: u16,
        cached: RwLock<Option<(SocketAddr, Instant)>>,
    },
}

impl Target {
    fn new(host: &str, port: u16) -> Self {
        match host.parse::<IpAddr>() {
            Ok(ip) => Self::Fixed(SocketAddr::new(ip, port)),
            Err(_) => Self::Host {
                host: host.to_owned(),
                port,
                cached: RwLock::new(None),
            },
        }
    }

    fn address(&self) -> Result<SocketAddr, PluginError> {
        let (host, port, cached) = match self {
            Self::Fixed(address) => return Ok(*address),
            Self::Host { host, port, cached } => (host, port, cached),
        };
        if let Some((address, at)) = *cached.read() {
            if at.elapsed() < RESOLVE_TTL {
                return Ok(address);
            }
        }
        let addresses: Vec<SocketAddr> = (host.as_str(), *port)
            .to_socket_addrs()
            .map_err(|e| PluginError::failed(format!("cannot resolve {host}: {e}")))?
            .collect();
        let address = addresses
            .iter()
            .find(|a| a.is_ipv4())
            .or(addresses.first())
            .copied()
            .ok_or_else(|| PluginError::failed(format!("cannot resolve {host}")))?;
        *cached.write() = Some((address, Instant::now()));
        Ok(address)
    }
}

struct SyslogSender {
    target: Target,
    ipv4_socket: OnceLock<UdpSocket>,
    ipv6_socket: OnceLock<UdpSocket>,
    priority: u16,
    format: MessageFormat,
    app_name: String,
    hostnames: Vec<String>,
}

impl SenderFactory for SyslogFactory {
    fn type_name(&self) -> &str {
        "Syslog"
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::required("ip", ArgType::String, "Syslog server address."),
            ArgSpec::required("port", ArgType::Integer, "Syslog server UDP port."),
            ArgSpec::optional("facility", ArgType::Integer, "Facility code 0-23 (default 1, user)."),
            ArgSpec::optional(
                "severity",
                ArgType::Integer,
                "Severity code 0-7 (default 6, informational).",
            ),
            ArgSpec::optional(
                "messageFormat",
                ArgType::String,
                "RFC_3164 (default), RFC_5424 or RFC_5425.",
            ),
            ArgSpec::required(
                "host",
                ArgType::List,
                "Device host names; one message is sent per host.",
            ),
            ArgSpec::optional("hostPrefix", ArgType::String, "Prefix added to every host name."),
        ]
    }

    fn create(&self, name: &str, args: &Args) -> Result<Box<dyn Sender>, PluginError> {
        let ip = arg_str(args, "ip").unwrap_or_default().trim();
        if ip.is_empty() {
            return Err(PluginError::invalid("ip"));
        }
        let port = arg_i64(args, "port")
            .and_then(Result::ok)
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(|| PluginError::invalid("port"))?;

        let code = |key: &str, max: i64, default: u8| match arg_i64(args, key) {
            Some(Ok(v)) if (0..=max).contains(&v) => v as u8,
            _ => default,
        };
        let facility = code("facility", 23, FACILITY_USER);
        let severity = code("severity", 7, SEVERITY_INFORMATIONAL);
        let prefix = arg_str(args, "hostPrefix").unwrap_or_default();

        Ok(Box::new(SyslogSender {
            target: Target::new(ip, port),
            ipv4_socket: OnceLock::new(),
            ipv6_socket: OnceLock::new(),
            priority: u16::from(facility) * 8 + u16::from(severity),
            format: MessageFormat::from_name(arg_str(args, "messageFormat").unwrap_or("RFC_3164")),
            app_name: name.to_owned(),
            hostnames: arg_string_list(args, "host")
                .unwrap_or_default()
                .into_iter()
                .map(|host| format!("{prefix}{host}"))
                .collect(),
        }))
    }
}

impl SyslogSender {
    /// UDP socket of the target's address family, opened on first use.
    fn socket(&self, target: SocketAddr) -> Result<&UdpSocket, PluginError> {
        let (cell, bind) = if target.is_ipv4() {
            (&self.ipv4_socket, "0.0.0.0:0")
        } else {
            (&self.ipv6_socket, "[::]:0")
        };
        if let Some(socket) = cell.get() {
            return Ok(socket);
        }
        let socket = UdpSocket::bind(bind).map_err(|e| PluginError::failed(format!("cannot open UDP socket: {e}")))?;
        Ok(cell.get_or_init(|| socket))
    }

    fn message(&self, hostname: &str, data: &str) -> String {
        let (pri, app) = (self.priority, &self.app_name);
        let rfc5424 = || {
            let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
            format!("<{pri}>1 {ts} {hostname} {app} - - - {data}")
        };
        match self.format {
            MessageFormat::Rfc3164 => {
                let ts = Local::now().format("%b %d %H:%M:%S");
                format!("<{pri}>{ts} {hostname} {app}: {data}")
            }
            MessageFormat::Rfc5424 => rfc5424(),
            MessageFormat::Rfc5425 => {
                let message = rfc5424();
                format!("{} {message}", message.len())
            }
        }
    }
}

impl Sender for SyslogSender {
    /// Succeeds when at least one host's message was sent.
    fn send(&self, data: &str) -> Result<(), PluginError> {
        let target = self.target.address()?;
        let socket = self.socket(target)?;
        let mut first_error = None;
        let mut sent = false;
        for hostname in &self.hostnames {
            match socket.send_to(self.message(hostname, data).as_bytes(), target) {
                Ok(_) => sent = true,
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        match first_error {
            Some(e) if !sent => Err(PluginError::failed(format!("syslog send failed: {e}"))),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use logmaker_plugin_api::check_args;
    use serde_json::{Value, json};

    use super::*;

    fn receive(format: Value, hosts: Value) -> Vec<String> {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut args = json!({
            "ip": "127.0.0.1",
            "port": server.local_addr().unwrap().port(),
            "host": hosts,
            "hostPrefix": "dev-",
            "facility": 4,
            "severity": 3,
        });
        if !format.is_null() {
            args["messageFormat"] = format;
        }
        let args = args.as_object().cloned().unwrap();
        check_args(&SyslogFactory.args(), &args).unwrap();
        let sender = SyslogFactory.create("app", &args).unwrap();
        sender.send("hello world").unwrap();

        let count = args["host"].as_array().unwrap().len();
        let mut buf = [0u8; 2048];
        (0..count)
            .map(|_| {
                let n = server.recv(&mut buf).unwrap();
                String::from_utf8(buf[..n].to_vec()).unwrap()
            })
            .collect()
    }

    #[test]
    fn sends_rfc3164_per_host() {
        let messages = receive(Value::Null, json!(["a", "b"]));
        assert_eq!(messages.len(), 2);
        // <35> = facility 4 * 8 + severity 3; "<35>Mar 07 05:04:09 dev-a app: hello world"
        for (message, host) in messages.iter().zip(["dev-a", "dev-b"]) {
            assert!(message.starts_with("<35>"), "{message}");
            assert!(message.ends_with(&format!(" {host} app: hello world")), "{message}");
            assert_eq!(
                message.len(),
                "<35>Mar 07 05:04:09 ".len() + host.len() + " app: hello world".len()
            );
        }
    }

    #[test]
    fn sends_rfc5424_and_5425() {
        let message = receive(json!("RFC_5424"), json!(["a"])).remove(0);
        assert!(message.starts_with("<35>1 "), "{message}");
        assert!(message.ends_with("Z dev-a app - - - hello world"), "{message}");

        let framed = receive(json!("RFC_5425"), json!(["a"])).remove(0);
        let (len, rest) = framed.split_once(' ').unwrap();
        assert_eq!(len.parse::<usize>().unwrap(), rest.len());
    }

    #[test]
    fn resolves_host_names_when_sending_and_prefers_ipv4() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let port = server.local_addr().unwrap().port();
        let args = json!({"ip": "localhost", "port": port, "host": ["h"], "messageFormat": "RFC_5424"});
        let sender = SyslogFactory.create("app", args.as_object().unwrap()).unwrap();
        sender.send("via name").unwrap();
        let mut buf = [0u8; 512];
        let n = server.recv(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).ends_with("h app - - - via name"));

        let args = json!({"ip": "no-such-host.invalid", "port": 514, "host": ["h"]});
        let unresolved = SyslogFactory.create("app", args.as_object().unwrap()).unwrap();
        assert!(matches!(unresolved.send("x"), Err(PluginError::Failed(m)) if m.contains("cannot resolve")));
    }

    #[test]
    fn rejects_bad_port_and_unknown_formats_fall_back() {
        let args = json!({"ip": "127.0.0.1", "port": 70000, "host": ["a"]})
            .as_object()
            .cloned()
            .unwrap();
        assert_eq!(
            SyslogFactory.create("x", &args).err(),
            Some(PluginError::invalid("port"))
        );
        assert_eq!(MessageFormat::from_name("nope"), MessageFormat::Rfc5424);
    }
}
