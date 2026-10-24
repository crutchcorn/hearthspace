use std::{
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Independent of the IPC receiver: an accepted launch always reaps its helper,
/// even if the shell closes or cancels its request while GIO is running.
pub fn run_opener(command: Command) -> Result<(), String> {
    run_opener_until(
        command,
        Instant::now() + Duration::from_secs(8),
        &AtomicBool::new(false),
    )
}

/// Bound acceptance, not application lifetime. A successfully spawned app is
/// reaped independently and is never killed when its launch request completes.
pub fn spawn_until(
    mut command: Command,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    check_pending(deadline, cancelled)?;
    let child = command
        .spawn()
        .map_err(|error| format!("Could not launch application: {error}"))?;
    crate::app_catalog::reap_child(child);
    Ok(())
}

struct Launcher(Child);

impl Drop for Launcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Preparation and child execution share the compositor's absolute deadline.
pub fn run_opener_until(
    mut command: Command,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    check_pending(deadline, cancelled)?;
    // Descendants inherit these handles. Piping output and waiting for EOF would
    // wait for the application rather than GIO's launch result.
    let mut child = Launcher(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("Could not open files: {error}"))?,
    );
    loop {
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|error| format!("Could not wait for file launcher: {error}"))?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Could not open files: the application launcher failed ({status})."
                ))
            };
        }
        check_pending(deadline, cancelled)?;
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn check_pending(deadline: Instant, cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("File launch was cancelled.".into());
    }
    if Instant::now() >= deadline {
        return Err("The application launcher did not respond in time.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_launcher_exit_failure() {
        assert!(
            run_opener(Command::new("/bin/false"))
                .unwrap_err()
                .contains("application launcher failed")
        );
    }

    #[test]
    fn stalled_launcher_is_killed_and_reaped_within_the_budget() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let start = Instant::now();
        assert!(
            run_opener_until(
                command,
                Instant::now() + Duration::from_millis(20),
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .contains("did not respond")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn an_expired_preparation_budget_cannot_spawn_an_application() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("launched");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        assert!(
            run_opener_until(
                command,
                Instant::now() - Duration::from_secs(1),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }

    #[test]
    fn cancelled_preparation_cannot_spawn_an_application() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("launched");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        assert!(
            run_opener_until(
                command,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }

    #[test]
    fn accepted_application_survives_its_launch_deadline_and_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("finished");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 0.1; touch \"$1\"", "fixture"])
            .arg(&marker);
        let cancelled = AtomicBool::new(false);
        spawn_until(
            command,
            Instant::now() + Duration::from_millis(50),
            &cancelled,
        )
        .unwrap();
        cancelled.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            marker.exists(),
            "acceptance cancellation killed the detached app"
        );
    }

    #[test]
    fn cancelled_detached_preparation_cannot_spawn() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("launched");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        assert!(
            spawn_until(
                command,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }
}
