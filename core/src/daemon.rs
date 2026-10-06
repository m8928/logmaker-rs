//! `--daemon`: run in the background, detached from the terminal (Unix).
//!
//! The launching process waits until the daemon reports that it is listening
//! or that startup failed, so `logmaker --daemon` exits non-zero when the
//! server cannot start. The daemon keeps the PID file locked while running,
//! which refuses a second daemon using the same file, and removes it on exit.

use std::path::Path;
use std::process::ExitCode;

/// Result of launching in daemon mode, seen by each process.
pub enum Launch {
    /// The launching process: exit with this code.
    Parent(ExitCode),
    /// The daemon: continue starting the server.
    Daemon(Startup),
}

/// Reports the end of startup to the launching process (no-op in the
/// foreground).
pub struct Startup {
    #[cfg(unix)]
    channel: Option<std::os::unix::net::UnixStream>,
}

impl Startup {
    pub fn foreground() -> Self {
        Self {
            #[cfg(unix)]
            channel: None,
        }
    }

    /// The server is listening.
    pub fn ready(&mut self) {
        self.send(&format!("ok {}", std::process::id()));
    }

    /// Startup failed; does nothing once `ready` was sent.
    pub fn failed(&mut self, error: &anyhow::Error) {
        self.send(&format!("error: {error:#}"));
    }

    fn send(&mut self, line: &str) {
        #[cfg(unix)]
        if let Some(mut channel) = self.channel.take() {
            use std::io::Write;
            let _ = writeln!(channel, "{line}");
        }
        #[cfg(not(unix))]
        let _ = line;
    }
}

/// Removes the PID file if it still holds this process's PID.
pub fn remove_pid_file(path: &Path) {
    let ours = std::fs::read_to_string(path).is_ok_and(|pid| pid.trim() == std::process::id().to_string());
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(not(unix))]
pub fn launch(_pid_file: &Path, _log_dir: &Path) -> anyhow::Result<Launch> {
    anyhow::bail!("--daemon is only supported on Unix")
}

/// Forks the daemon. Must be called before any thread is started.
///
/// The daemon keeps the current directory, so relative paths keep their
/// meaning; its stdout and stderr go to `log_dir/logmaker.out` (or nowhere
/// when file logging is disabled).
#[cfg(unix)]
pub fn launch(pid_file: &Path, log_dir: &Path) -> anyhow::Result<Launch> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    use anyhow::Context;
    use daemonize::{Daemonize, Outcome, Stdio};

    let output_path = (!log_dir.as_os_str().is_empty()).then(|| log_dir.join("logmaker.out"));
    let (stdout, stderr) = match &output_path {
        Some(path) => {
            std::fs::create_dir_all(log_dir).with_context(|| format!("cannot create {}", log_dir.display()))?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("cannot open {}", path.display()))?;
            (Stdio::from(file.try_clone()?), Stdio::from(file))
        }
        None => (Stdio::devnull(), Stdio::devnull()),
    };
    let (parent_end, daemon_end) = UnixStream::pair().context("cannot create startup channel")?;
    let daemonize = Daemonize::new()
        .pid_file(pid_file)
        .working_directory(std::env::current_dir().context("cannot read current directory")?)
        .stdout(stdout)
        .stderr(stderr);

    match daemonize.execute() {
        Outcome::Parent(Ok(_)) => {
            drop(daemon_end);
            Ok(Launch::Parent(wait_for_startup(
                parent_end,
                pid_file,
                output_path.as_deref(),
            )))
        }
        Outcome::Parent(Err(e)) => Err(anyhow::anyhow!("cannot start daemon: {e}")),
        Outcome::Child(Ok(_)) => {
            drop(parent_end);
            Ok(Launch::Daemon(Startup {
                channel: Some(daemon_end),
            }))
        }
        Outcome::Child(Err(e)) => {
            // Typically the PID file is locked by a running daemon.
            let mut channel = daemon_end;
            let _ = writeln!(
                channel,
                "error: {e} ({}; is another logmaker daemon using it?)",
                pid_file.display()
            );
            std::process::exit(1);
        }
    }
}

/// How long the launching process waits for the daemon to start listening.
#[cfg(unix)]
const STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(unix)]
fn wait_for_startup(channel: std::os::unix::net::UnixStream, pid_file: &Path, output: Option<&Path>) -> ExitCode {
    use std::io::{BufRead, BufReader};

    let _ = channel.set_read_timeout(Some(STARTUP_TIMEOUT));
    let mut line = String::new();
    let read = BufReader::new(channel).read_line(&mut line);
    let line = line.trim_end();
    let see_output = output.map_or_else(String::new, |path| format!(" (see {})", path.display()));
    match read {
        Ok(_) if line.starts_with("ok ") => {
            let pid = line.trim_start_matches("ok ");
            println!(
                "logmaker started in the background (pid {pid}, pid file {})",
                pid_file.display()
            );
            ExitCode::SUCCESS
        }
        Ok(_) if line.starts_with("error: ") => {
            eprintln!(
                "logmaker: daemon failed to start: {}",
                line.trim_start_matches("error: ")
            );
            ExitCode::FAILURE
        }
        Ok(_) => {
            eprintln!("logmaker: daemon exited during startup{see_output}");
            ExitCode::FAILURE
        }
        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
            eprintln!(
                "logmaker: daemon did not finish starting within {}s; it may still be running (pid file {}){see_output}",
                STARTUP_TIMEOUT.as_secs(),
                pid_file.display()
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("logmaker: cannot read daemon startup status: {e}");
            ExitCode::FAILURE
        }
    }
}
