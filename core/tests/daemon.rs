//! `logmaker --daemon`: starts in the background, reports startup failures to
//! the caller, guards the PID file and removes it on shutdown.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn logmaker(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_logmaker"))
        .current_dir(dir)
        .args(["--bind", "127.0.0.1", "--data-root", "data", "--plugin-root", "plugins"])
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn health(port: u16) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .write_all(b"GET /actuator/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    condition()
}

#[test]
fn runs_in_the_background_and_reports_startup_failures() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port().to_string();

    let started = logmaker(dir.path(), &["--daemon", "--port", &port]);
    assert!(started.status.success(), "{}", text(&started.stderr));
    assert!(text(&started.stdout).starts_with("logmaker started in the background (pid "));
    let pid = std::fs::read_to_string(dir.path().join("logmaker.pid"))
        .unwrap()
        .trim()
        .to_owned();
    assert!(health(port.parse().unwrap()).is_some_and(|r| r.contains(r#"{"status":"UP"}"#)));

    // The running daemon keeps its PID file locked.
    let second = logmaker(dir.path(), &["-d", "--port", &free_port().to_string()]);
    assert!(!second.status.success());
    assert!(
        text(&second.stderr).contains("unable to lock pid file"),
        "{}",
        text(&second.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("logmaker.pid")).unwrap().trim(),
        pid
    );

    // Startup errors reach the caller instead of being lost in the background.
    let busy = logmaker(dir.path(), &["-d", "--port", &port, "--pid-file", "busy.pid"]);
    assert!(!busy.status.success());
    assert!(
        text(&busy.stderr).contains("cannot listen on 127.0.0.1:"),
        "{}",
        text(&busy.stderr)
    );
    assert!(!dir.path().join("busy.pid").exists());

    assert!(Command::new("kill").args(["-TERM", &pid]).status().unwrap().success());
    assert!(wait_until(Duration::from_secs(10), || !dir
        .path()
        .join("logmaker.pid")
        .exists()));
    assert!(wait_until(Duration::from_secs(5), || health(port.parse().unwrap()).is_none()));
    let logs: String = std::fs::read_dir(dir.path().join("logs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default())
        .collect();
    assert!(logs.contains("shutting down"));
}
