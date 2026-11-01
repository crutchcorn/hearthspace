use std::{
    io,
    net::Shutdown,
    os::unix::net::UnixStream,
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use calloop::EventLoop;
use smithay::{
    wayland::compositor::CompositorHandler,
    xwayland::{XWayland, XWaylandClientData, XWaylandEvent},
};
use tracing::info;
use wayland_server::{Client, DisplayHandle};

use super::{App, dependency::Dependency};
use crate::compositor::CalloopData;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

pub(in crate::compositor) struct Startup {
    dependency: Dependency,
    client: Client,
    deadline: Instant,
    ready: bool,
    failure: Option<String>,
}

impl Startup {
    fn terminate(&self) -> io::Result<()> {
        self.client
            .get_data::<XWaylandClientData>()
            .ok_or_else(|| self.dependency.error("owned child data is missing"))?
            .terminate_and_wait()
            .map(|_| ())
            .map_err(|error| {
                self.dependency
                    .error(format!("could not terminate and reap owned child: {error}"))
            })
    }

    fn remaining(&self) -> io::Result<Option<Duration>> {
        if let Some(error) = &self.failure {
            return Err(self.dependency.error(error));
        }
        if self.ready {
            return Ok(None);
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .map(Some)
            .ok_or_else(|| {
                self.dependency
                    .error("timed out waiting for Xwayland and its window manager")
            })
    }
}

impl App {
    pub(in crate::compositor) fn start_xwayland(&mut self, dh: &DisplayHandle) -> io::Result<()> {
        let dependency = Dependency::current()?;
        dependency.validate()?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let (xwayland, client) = XWayland::spawn_with_executable(
            dh,
            &dependency.binary,
            None,
            std::iter::empty::<(String, String)>(),
            ["-surface-input-targets"],
            true,
            Stdio::inherit(),
            Stdio::inherit(),
            |_| (),
        )
        .map_err(|error| dependency.error(error))?;
        self.xwayland_startup = Some(Startup {
            dependency: dependency.clone(),
            client: client.clone(),
            deadline,
            ready: false,
            failure: None,
        });
        let display_handle = dh.clone();
        self.loop_handle.insert_source(xwayland, move |event, _, data| {
            let result = match event {
                XWaylandEvent::Ready { x11_socket, display_number } => {
                    data.state.client_compositor_state(&client).set_client_scale(1.0);
                    data.state.start_xwm(&display_handle, x11_socket, client.clone(), display_number)
                        .map_err(|error| format!("failed to initialize XWM: {error}"))
                }
                XWaylandEvent::Error => Err("Xwayland exited during startup".to_owned()),
            };
            let startup = data.state.xwayland_startup.as_mut().expect("Xwayland startup state");
            match result {
                Ok(()) if Instant::now() < startup.deadline => {
                    startup.ready = true;
                    info!(binary = %startup.dependency.binary.display(), "Xwayland and its window manager are ready");
                }
                Ok(()) => startup.failure = Some("XWM initialization exceeded startup deadline".into()),
                Err(error) => startup.failure = Some(error),
            }
            if startup.ready {
                data.state.publish_xwayland_display();
            }
        }).map_err(|error| dependency.error(error))?;
        Ok(())
    }

    pub(super) fn xwm_startup_deadline(&self) -> Instant {
        self.xwayland_startup
            .as_ref()
            .expect("Xwayland startup state")
            .deadline
    }

    pub(in crate::compositor) fn xwayland_ready(&self) -> bool {
        self.xwayland_startup
            .as_ref()
            .is_some_and(|startup| startup.ready)
            && self.xdisplay.is_some()
            && self.xwm.is_some()
    }
}

pub(in crate::compositor) fn wait(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = wait_ready(event_loop, data) {
        if let Some(startup) = &data.state.xwayland_startup
            && let Err(cleanup) = startup.terminate()
        {
            return Err(format!("{error}; cleanup failed: {cleanup}").into());
        }
        return Err(error);
    }
    Ok(())
}

fn wait_ready(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Some(startup) = &data.state.xwayland_startup {
        // Smithay keeps its existing displayfd behavior. The Wayland backend
        // independently retires the private client when the child exits, even
        // when no XWaylandEvent::Error was emitted for displayfd EOF.
        if data
            .display
            .handle()
            .backend_handle()
            .get_client_data(startup.client.id())
            .is_err()
        {
            return Err(startup
                .dependency
                .error("Xwayland exited during startup")
                .into());
        }
        let Some(remaining) = startup.remaining()? else {
            break;
        };
        if !data.running {
            return Err(startup
                .dependency
                .error("compositor stopped before XWM readiness")
                .into());
        }
        let dependency = startup.dependency.clone();
        event_loop
            .dispatch(Some(remaining), data)
            .map_err(|error| dependency.error(error))?;
        data.display
            .flush_clients()
            .map_err(|error| dependency.error(error))?;
    }
    if let Some(startup) = &data.state.xwayland_startup
        && (!data.running || !data.state.xwayland_ready())
    {
        return Err(startup
            .dependency
            .error("XWM disconnected before startup completed")
            .into());
    }
    Ok(())
}

/// x11rb performs synchronous initialization and switches the socket to
/// nonblocking mode, so SO_RCVTIMEO alone cannot bound its internal poll.
/// Shut down only this startup connection if the absolute deadline expires.
/// Cancellation and join ensure no watchdog can close a later healthy XWM.
pub(super) struct ConnectionDeadline {
    cancel: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ConnectionDeadline {
    pub(super) fn new(socket: &UnixStream, deadline: Instant) -> io::Result<Self> {
        let socket = socket.try_clone()?;
        let (cancel, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("xwm-startup-deadline".into())
            .spawn(move || {
                if matches!(
                    receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    let _ = socket.shutdown(Shutdown::Both);
                }
            })?;
        Ok(Self {
            cancel: Some(cancel),
            worker: Some(worker),
        })
    }
}

impl Drop for ConnectionDeadline {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;
