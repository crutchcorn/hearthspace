//! Independent StatusNotifierItem/DBusMenu peer for owned-session acceptance.
#[path = "tray_fixture/item.rs"]
mod item;
#[path = "tray_fixture/menu.rs"]
mod menu;
#[path = "tray_fixture/state.rs"]
mod state;

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use state::{Shared, State};
use zbus::{Connection, object_server::SignalEmitter};

const NAME: &str = "org.hearthspace.TrayFixture";

async fn register(connection: &Connection) -> zbus::Result<()> {
    let watcher = zbus::Proxy::new(
        connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await?;
    watcher.call("RegisterStatusNotifierItem", &(NAME,)).await
}

async fn command(connection: &Connection, shared: &Shared, command: &str) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(connection, "/StatusNotifierItem")?;
    match command {
        "register" => {
            connection.request_name(NAME).await?;
            register(connection).await?;
        }
        "unregister" => {
            connection.release_name(NAME).await?;
        }
        "blue" | "red" => {
            {
                let mut state = shared.lock().unwrap();
                state.blue = command == "blue";
                state.named = false;
            }
            item::Item::new_icon(&emitter).await?;
        }
        "named-red" | "named-blue" => {
            shared
                .lock()
                .unwrap()
                .rewrite_icon(command == "named-blue")
                .map_err(|error| zbus::Error::Failure(error.to_string()))?;
            item::Item::new_icon(&emitter).await?;
        }
        "passive" | "active" | "attention" => {
            let status = match command {
                "passive" => "Passive",
                "attention" => "NeedsAttention",
                _ => "Active",
            };
            shared.lock().unwrap().status = status.into();
            item::Item::new_status(&emitter, status).await?;
        }
        "menu-update" => {
            let revision = {
                let mut state = shared.lock().unwrap();
                state.updated = true;
                state.revision += 1;
                state.revision
            };
            menu::Menu::layout_updated(&SignalEmitter::new(connection, "/Menu")?, revision, 0)
                .await?;
        }
        "menu-only" | "no-menu" => {
            {
                let mut state = shared.lock().unwrap();
                state.menu_only = command == "menu-only";
                state.has_menu = command != "no-menu";
            }
            // Both optional properties participate in the standard PropertiesChanged signal.
            let iface = connection
                .object_server()
                .interface::<_, item::Item>("/StatusNotifierItem")
                .await?;
            let guard = iface.get().await;
            guard.item_is_menu_changed(iface.signal_emitter()).await?;
            guard.menu_changed(iface.signal_emitter()).await?;
        }
        "quit" => {}
        _ => {
            return Err(zbus::Error::Failure(format!(
                "unknown fixture command: {command}"
            )));
        }
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let socket = PathBuf::from(args.next().ok_or("expected control socket")?);
    let log = PathBuf::from(args.next().ok_or("expected action log")?);
    if args.next().is_some() || std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return Err("requires exactly SOCKET LOG and an explicit session bus".into());
    }
    let shared = Arc::new(Mutex::new(State::new(log)?));
    let connection = zbus::connection::Builder::session()?
        .method_timeout(std::time::Duration::from_secs(2))
        .name(NAME)?
        .serve_at("/StatusNotifierItem", item::Item(shared.clone()))?
        .serve_at("/Menu", menu::Menu(shared.clone()))?
        .build()
        .await?;
    // A native watchdog can tear down the owned bus after the test runner dies.
    // Do not leave the independent app fixture waiting on its control socket.
    let disconnect = connection.clone();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("fixture disconnect runtime")
            .block_on(disconnect.closed());
        std::process::exit(0);
    });
    for attempt in 0..20 {
        match register(&connection).await {
            Ok(()) => break,
            Err(error) if attempt == 19 => return Err(error.into()),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    // zbus's async-io executor runs independently. The fixture's control channel
    // deliberately remains a separate, bounded local stream, not a tray API.
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
        let mut line = String::new();
        BufReader::new(&stream).take(128).read_line(&mut line)?;
        let name = line.trim();
        let result = command(&connection, &shared, name).await;
        let response = match result {
            Ok(()) => serde_json::json!({"ok": true}),
            Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
        };
        writeln!(stream, "{response}")?;
        if name == "quit" {
            break;
        }
    }
    std::fs::remove_file(socket)?;
    Ok(())
}
