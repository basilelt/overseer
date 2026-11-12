use std::process::{Command, Output, Stdio};
use std::time::Duration;

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Watchdog {
    tx: Option<std::sync::mpsc::Sender<()>>,
    dog: Option<std::thread::JoinHandle<bool>>,
}

impl Watchdog {
    pub fn guard(cmd: &mut Command) -> &mut Command {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(cmd, 0);
        cmd
    }

    pub fn start(pid: u32, limit: Duration) -> Watchdog {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let dog = std::thread::spawn(move || {
            let fired = matches!(
                rx.recv_timeout(limit),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            );
            if fired {
                kill_group(pid);
            }
            fired
        });
        Watchdog {
            tx: Some(tx),
            dog: Some(dog),
        }
    }

    pub fn finish(mut self) -> bool {
        self.stop()
    }

    fn stop(&mut self) -> bool {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(());
        }
        self.dog
            .take()
            .map(|d| d.join().unwrap_or(false))
            .unwrap_or(false)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop();
    }
}

fn kill_group(pid: u32) {
    #[cfg(unix)]
    if let Ok(p) = i32::try_from(pid) {
        unsafe { libc::kill(-p, libc::SIGKILL) };
    }
    #[cfg(not(unix))]
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[derive(Debug)]
pub enum RunError {
    Spawn(std::io::Error),
    TimedOut(Duration),
    Wait(std::io::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Spawn(e) | RunError::Wait(e) => write!(f, "{e}"),
            RunError::TimedOut(d) => write!(f, "timed out after {} s", d.as_secs().max(1)),
        }
    }
}

pub fn run_within(cmd: &mut Command, limit: Duration) -> Result<Output, RunError> {
    run_tracked(cmd, limit, |_| ())
}

pub fn run_tracked<T>(
    cmd: &mut Command,
    limit: Duration,
    track: impl FnOnce(u32) -> T,
) -> Result<Output, RunError> {
    let child = Watchdog::guard(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunError::Spawn)?;
    let _tracked = track(child.id());
    let dog = Watchdog::start(child.id(), limit);
    let out = child.wait_with_output();
    if dog.finish() {
        return Err(RunError::TimedOut(limit));
    }
    out.map_err(RunError::Wait)
}

pub fn output_within(cmd: &mut Command, limit: Duration) -> Result<Output, String> {
    run_within(cmd, limit).map_err(|e| e.to_string())
}

pub fn run_stdin(cmd: &mut Command, input: Vec<u8>, limit: Duration) -> Result<Output, RunError> {
    use std::io::Write;
    let mut child = Watchdog::guard(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(RunError::Spawn)?;
    let dog = Watchdog::start(child.id(), limit);
    if let Some(mut i) = child.stdin.take() {
        std::thread::spawn(move || i.write_all(&input));
    }
    let out = child.wait_with_output();
    if dog.finish() {
        return Err(RunError::TimedOut(limit));
    }
    out.map_err(RunError::Wait)
}

pub fn stdout_ok(cmd: &mut Command, limit: Duration) -> Option<String> {
    let out = output_within(cmd, limit).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn stdout_or(
    cmd: &mut Command,
    limit: Duration,
    name: &str,
    failure: impl FnOnce(&Output) -> String,
) -> Result<String, String> {
    let out = output_within(cmd, limit).map_err(|e| format!("{name}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(failure(&out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracked_child_is_registered_until_it_has_been_waited_for() {
        struct Guard(std::sync::Arc<std::sync::atomic::AtomicU32>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(0, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let s = seen.clone();
        let out = run_tracked(
            Command::new("echo").arg("hi"),
            Duration::from_secs(5),
            |pid| {
                s.store(pid, std::sync::atomic::Ordering::SeqCst);
                Guard(s.clone())
            },
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 0);
        let missing = run_tracked(
            &mut Command::new("sb-no-such-bin"),
            Duration::from_secs(1),
            |_| panic!("never spawned"),
        );
        assert!(matches!(missing, Err(RunError::Spawn(_))));
    }

    #[cfg(unix)]
    #[test]
    fn stdin_is_fed_to_the_child_and_a_hung_child_times_out() {
        let big = vec![b'x'; 1 << 20];
        let out = run_stdin(
            &mut Command::new("cat"),
            big.clone(),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out.stdout.len(), big.len());
        let slow = run_stdin(
            Command::new("sleep").arg("30"),
            vec![],
            Duration::from_millis(200),
        );
        assert!(matches!(slow, Err(RunError::TimedOut(_))));
        let missing = run_stdin(
            &mut Command::new("sb-no-such-bin"),
            vec![],
            Duration::from_secs(10),
        );
        assert!(matches!(missing, Err(RunError::Spawn(_))));
    }

    #[cfg(unix)]
    #[test]
    fn system_command_helpers_keep_each_callers_failure_shape() {
        let ok = |c: &mut Command| stdout_ok(c, Duration::from_secs(5));
        assert_eq!(ok(Command::new("echo").arg("a")).as_deref(), Some("a\n"));
        assert_eq!(ok(&mut Command::new("false")), None);
        assert_eq!(ok(&mut Command::new("sb-no-such-bin")), None);
        let or = |c: &mut Command| {
            stdout_or(c, Duration::from_secs(5), "tool", |o| {
                format!("exit {}", o.status.code().unwrap_or(-1))
            })
        };
        assert_eq!(or(Command::new("echo").arg("a")).unwrap(), "a\n");
        assert_eq!(or(&mut Command::new("false")).unwrap_err(), "exit 1");
        let e = or(&mut Command::new("sb-no-such-bin")).unwrap_err();
        assert!(e.starts_with("tool: "), "{e}");
    }
}
