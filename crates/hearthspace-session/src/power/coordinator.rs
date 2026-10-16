use std::{
    io,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::time::Instant;

use super::{
    api::{Reply, Request, Shared},
    control,
    logind::{Event, Logind, Snapshot},
    policy::{LidPolicy, LidState},
    status::{Status, external_display},
};

mod manual;
use manual::Manual;

pub(super) async fn run(
    runtime: PathBuf,
    socket: PathBuf,
    shared: Shared,
    mut requests: tokio::sync::mpsc::Receiver<Request>,
) {
    let mut status = Status {
        phase: "connecting",
        ..Status::default()
    };
    loop {
        status.save(&runtime);
        let result = match Logind::connect().await {
            Ok(mut logind) => {
                connected(
                    &mut logind,
                    &runtime,
                    &socket,
                    &mut status,
                    &shared,
                    &mut requests,
                )
                .await
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            // Lost transport also loses our inhibitors. Shield an established
            // session before reconnecting; never silently continue unprotected.
            if status.connected {
                let _ = control::command(&socket, "lock-shield").await;
            }
            status.error(error);
        }
        shared.update(Reply::unavailable("Power service is disconnected"));
        while requests.try_recv().is_ok() {}
        status.connected = false;
        status.sleep_inhibited = false;
        status.lid_owned = false;
        status.save(&runtime);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

struct Cycle {
    delay: Option<OwnedFd>,
    lid: Option<OwnedFd>,
    policy: LidPolicy,
    sleeping: bool,
    preparing: Option<Instant>,
    recovering: Option<Instant>,
    resume_accepted: bool,
    lid_request: Option<Instant>,
    hint: Option<bool>,
    manual: Manual,
}

async fn connected(
    logind: &mut Logind,
    runtime: &Path,
    socket: &Path,
    status: &mut Status,
    shared: &Shared,
    requests: &mut tokio::sync::mpsc::Receiver<Request>,
) -> io::Result<()> {
    let first = logind.snapshot().await?;
    if !first.local_seat {
        return Err(io::Error::other(
            "power management requires this process's local VT session",
        ));
    }
    let mut cycle = Cycle {
        delay: Some(logind.take_sleep_delay().await?),
        lid: None,
        policy: LidPolicy::default(),
        sleeping: false,
        preparing: None,
        recovering: None,
        resume_accepted: true,
        lid_request: None,
        hint: None,
        manual: Manual::default(),
    };
    cycle.policy.reset();
    let reconnecting = !status.session_id.is_empty();
    status.connected = true;
    status.session_id = first.session_id.clone();
    status.phase = "awake";
    status.last_error = None;
    tracing::info!(session = %first.session_id, path = %first.session_path, "power coordinator connected; sleep delay armed");
    if reconnecting && !first.preparing_for_sleep {
        // A bus outage can hide an entire sleep/wake cycle. Rebuild conservatively
        // without claiming that a physical suspend was observed.
        status.phase = "recovering";
        status.lock_confirmed = false;
        status.recovery_ready = false;
        cycle.recovering = Some(Instant::now() + Duration::from_secs(12));
        cycle.resume_accepted = false;
        for command in ["lock-shield", "session-resume"] {
            match control::command(socket, command).await {
                Ok(_) if command == "session-resume" => cycle.resume_accepted = true,
                Ok(_) => {}
                Err(error) => status.error(error),
            }
        }
    }
    let mut snapshot = first;
    loop {
        if snapshot.preparing_for_sleep != cycle.sleeping {
            cycle
                .sleep_event(
                    snapshot.preparing_for_sleep,
                    &snapshot,
                    logind,
                    socket,
                    status,
                )
                .await?;
        }
        cycle.tick(&snapshot, logind, socket, status).await?;
        status.sleep_inhibited = cycle.delay.is_some();
        status.lid_owned = cycle.lid.is_some();
        status.save(runtime);
        shared.update(cycle.manual_capability(logind, &snapshot, status).await);
        tokio::select! {
            Some(request) = requests.recv() => {
                cycle.request_suspend(request, logind, socket, status).await;
            },
            event = logind.next_event() => match event? {
                Event::PrepareForSleep(preparing) => {
                    cycle.sleep_event(preparing, &snapshot, logind, socket, status).await?;
                    snapshot.preparing_for_sleep = preparing;
                }
                Event::SessionLock => {
                    if let Err(error) = control::command(socket, "lock-shield").await {
                        status.error(error);
                    }
                }
            },
            () = tokio::time::sleep(Duration::from_millis(250)) => {
                snapshot = logind.snapshot().await?;
            }
        }
    }
}

impl Cycle {
    async fn sleep_event(
        &mut self,
        preparing: bool,
        snapshot: &Snapshot,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
    ) -> io::Result<()> {
        if preparing == self.sleeping {
            return Ok(());
        }
        self.sleeping = preparing;
        self.manual.sleep_event();
        self.lid_request = None;
        if preparing {
            status.sleep_count += 1;
            status.phase = "preparing-sleep";
            self.preparing = Some(
                Instant::now()
                    + snapshot
                        .delay_max
                        .saturating_sub(Duration::from_millis(250)),
            );
            self.recovering = None;
            tracing::info!("PrepareForSleep(true): shielding session before releasing sleep delay");
            if let Err(error) = control::command(socket, "prepare-sleep").await {
                status.error(error);
            }
        } else {
            status.resume_count += 1;
            status.phase = "recovering";
            self.preparing = None;
            self.delay = Some(logind.take_sleep_delay().await?);
            self.recovering = Some(Instant::now() + Duration::from_secs(12));
            status.lock_confirmed = false;
            status.recovery_ready = false;
            tracing::info!(
                "PrepareForSleep(false): sleep delay rearmed; recovering display and input"
            );
            self.resume_accepted = false;
            match control::command(socket, "session-resume").await {
                Ok(_) => self.resume_accepted = true,
                Err(error) => status.error(error),
            }
        }
        Ok(())
    }

    async fn tick(
        &mut self,
        snapshot: &Snapshot,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
    ) -> io::Result<()> {
        self.tick_with_display_probe(snapshot, logind, socket, status, external_display)
            .await
    }

    async fn tick_with_display_probe(
        &mut self,
        snapshot: &Snapshot,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
        external: impl Fn() -> bool,
    ) -> io::Result<()> {
        status.active = snapshot.active;
        if snapshot.active && self.lid.is_none() {
            match logind.take_lid_control().await {
                Ok(fd) => self.lid = Some(fd),
                Err(error) => status.error(error),
            }
        } else if !snapshot.active {
            self.lid = None;
            self.lid_request = None;
        }
        let closing = self.policy.update(LidState {
            closed: snapshot.lid_closed,
            active: snapshot.active,
            docked: snapshot.docked,
            external_display: external(),
            controls_lid: self.lid.is_some(),
        });
        if closing && !self.sleeping && !self.manual.busy() {
            tracing::info!("lid closed in active undocked session; preparing lock");
            self.lid_request = Some(Instant::now() + Duration::from_secs(10));
            if let Err(error) = control::command(socket, "lock-shield").await {
                status.error(error);
            }
        }
        let compositor = match control::command(socket, "status").await {
            Ok(value) => value,
            Err(error) => {
                status.error(error);
                serde_json::Value::Null
            }
        };
        status.lock_confirmed = control::confirmed(&compositor);
        status.recovery_ready = compositor["recovery_ready"] == true;
        if let Some(deadline) = self.preparing {
            if status.lock_confirmed {
                self.preparing = None;
                self.delay = None;
                status.phase = "sleeping";
                tracing::info!("session lock presented; sleep delay released");
            } else if Instant::now() >= deadline {
                self.preparing = None;
                status.error("lock presentation missed logind's delay budget; shield retained, sleep may proceed");
                // Do not advertise an expired delay as effective protection.
                self.delay = None;
            }
        }
        if let Some(deadline) = self.recovering {
            if !snapshot.active {
                // A different VT owns the hardware. Start the presentation
                // budget only after our own session regains the seat.
                self.recovering = Some(Instant::now() + Duration::from_secs(12));
                status.phase = "recovering";
            } else if self.resume_accepted
                && status.recovery_ready
                && (status.lock_confirmed
                    || (compositor["active"] == true && compositor["locked"] == false))
            {
                self.recovering = None;
                status.phase = "awake";
                status.last_error = None;
                tracing::info!("resume complete: presented lock or authenticated unlock observed");
            } else if Instant::now() >= deadline {
                self.recovering = None;
                status.error("resume presentation timed out; session remains shielded");
            }
        }
        self.tick_manual(snapshot, logind, socket, status, &compositor)
            .await;
        if let Some(deadline) = self.lid_request {
            // Recheck eligibility after the asynchronous lock presentation.
            if Instant::now() >= deadline {
                self.lid_request = None;
                status.error("lid suspend cancelled: lock was not presented in time");
            } else if !snapshot.active || !snapshot.lid_closed || snapshot.docked || external() {
                self.lid_request = None;
            } else if status.lock_confirmed {
                self.lid_request = None;
                let current = logind.snapshot().await?;
                if !(current.active
                    && current.local_seat
                    && current.lid_closed
                    && !current.docked
                    && !external()
                    && self.lid.is_some())
                {
                    return Ok(());
                }
                if let Err(error) = logind.suspend().await {
                    // The call may already have initiated sleep; never retry a
                    // timed-out suspend while the lid remains closed.
                    status.error(error);
                }
            }
        }
        // Hint updates are informational; never delay releasing a presented
        // shield for them and never mark unlocked on a failed status read.
        if let Some(locked) = compositor["locked"].as_bool()
            && self.hint != Some(locked)
        {
            match logind.set_locked_hint(locked).await {
                Ok(()) => self.hint = Some(locked),
                Err(error) => status.error(error),
            }
        }
        if !self.sleeping
            && self.preparing.is_none()
            && self.recovering.is_none()
            && self.lid_request.is_none()
            && !self.manual.busy()
            && self.delay.is_some()
            && self.resume_accepted
            && status.recovery_ready
            && (!snapshot.active || (self.lid.is_some() && compositor["active"] == true))
            && (compositor["locked"] == false || status.lock_confirmed)
            && self.hint == compositor["locked"].as_bool()
        {
            // Startup and temporary transport failures must not leave a healthy
            // session permanently degraded after all required checks recover.
            status.phase = "awake";
            status.last_error = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
