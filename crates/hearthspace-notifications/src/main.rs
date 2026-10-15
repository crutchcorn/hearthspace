use std::{
    env, fs,
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod control;
mod dbus;
mod model;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime =
        PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is required")?);
    let socket = runtime.join("hearthspace-notifications.sock");
    let state = Arc::new(Mutex::new(model::State::default()));
    let stopped = Arc::new(AtomicBool::new(false));
    let stop_flag = stopped.clone();
    ctrlc::set_handler(move || stop_flag.store(true, Ordering::SeqCst))?;
    let connection = tokio::time::timeout(
        Duration::from_secs(3),
        zbus::connection::Builder::session()?
            .max_queued(64)
            .allow_name_replacements(false)
            .replace_existing_names(false)
            .method_timeout(Duration::from_secs(2))
            .name(dbus::NAME)?
            .serve_at(dbus::PATH, dbus::Notifications(state.clone()))?
            .build(),
    )
    .await??;
    // Requesting the well-known name fails if another daemon owns it. Only then
    // remove our stale control socket, which can remain after an earlier crash.
    match fs::remove_file(&socket) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let mut server = control::Server::default();
    while !stopped.load(Ordering::SeqCst) {
        server.poll(&listener, &state)?;
        let events = {
            let mut state = state.lock().unwrap();
            state.expire(Instant::now());
            state.take_events()
        };
        for event in events {
            tokio::time::timeout(Duration::from_millis(250), send_event(&connection, event))
                .await??;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    fs::remove_file(socket)?;
    Ok(())
}

async fn send_event(connection: &zbus::Connection, event: model::Event) -> zbus::Result<()> {
    match event {
        model::Event::Closed { sender, id, reason } => {
            connection
                .emit_signal(
                    Some(sender.as_str()),
                    dbus::PATH,
                    dbus::NAME,
                    "NotificationClosed",
                    &(id, reason),
                )
                .await
        }
        model::Event::Action { sender, id, key } => {
            connection
                .emit_signal(
                    Some(sender.as_str()),
                    dbus::PATH,
                    dbus::NAME,
                    "ActionInvoked",
                    &(id, key),
                )
                .await
        }
    }
}
