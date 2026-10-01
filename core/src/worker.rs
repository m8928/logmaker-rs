//! Background threads that can be stopped promptly, including while sleeping.

use std::any::Any;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

/// How long stop requests wait for a runtime thread to exit.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct StopSignal {
    stopped: AtomicBool,
    lock: Mutex<()>,
    wake: Condvar,
}

impl StopSignal {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _guard = self.lock.lock();
        self.wake.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// Sleeps up to `duration`. Returns `false` if stopped before or during
    /// the sleep.
    pub fn sleep(&self, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        let mut guard = self.lock.lock();
        while !self.stopped.load(Ordering::SeqCst) {
            if self.wake.wait_until(&mut guard, deadline).timed_out() {
                return !self.stopped.load(Ordering::SeqCst);
            }
        }
        false
    }
}

/// A named thread paired with its stop signal.
pub struct Worker {
    signal: Arc<StopSignal>,
    handle: JoinHandle<()>,
}

impl Worker {
    pub fn spawn(name: &str, body: impl FnOnce(&StopSignal) + Send + 'static) -> io::Result<Self> {
        let signal = Arc::new(StopSignal::default());
        let thread_signal = Arc::clone(&signal);
        let thread_name: String = name.chars().filter(|&c| c != '\0').collect();
        let label = thread_name.clone();
        let handle = std::thread::Builder::new().name(thread_name).spawn(move || {
            if let Err(panic) = catch_unwind(AssertUnwindSafe(|| body(&thread_signal))) {
                tracing::error!("thread {label} panicked: {}", panic_message(panic.as_ref()));
            }
        })?;
        Ok(Self { signal, handle })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Signals the thread and waits up to `timeout` for it to exit. Returns
    /// `false` if it is still running.
    pub fn stop(&self, timeout: Duration) -> bool {
        self.signal.stop();
        let deadline = Instant::now() + timeout;
        while !self.handle.is_finished() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_interrupts_sleep() {
        let worker = Worker::spawn("sleeper", |signal| {
            signal.sleep(Duration::from_secs(60));
        })
        .unwrap();
        let started = Instant::now();
        assert!(worker.stop(Duration::from_secs(5)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn sleep_reports_completion() {
        let signal = StopSignal::default();
        assert!(signal.sleep(Duration::from_millis(1)));
        signal.stop();
        assert!(!signal.sleep(Duration::from_secs(60)));
    }

    #[test]
    fn panicking_body_finishes() {
        let worker = Worker::spawn("panics", |_| panic!("boom")).unwrap();
        assert!(worker.stop(Duration::from_secs(5)));
    }
}
