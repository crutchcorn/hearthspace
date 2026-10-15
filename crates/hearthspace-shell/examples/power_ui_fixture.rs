//! Isolated UPower peer for shell acceptance; never connects to a host bus.
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Deserialize;
use zbus::{Connection, connection::Builder};

const ROOT: &str = "/org/freedesktop/UPower";
const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    present: bool,
    kind: u32,
    percentage: f64,
    state: u32,
    warning: u32,
    on_battery: bool,
}

type Shared = Arc<Mutex<State>>;
struct Power(Shared);
struct Device(Shared);

#[zbus::interface(name = "org.freedesktop.UPower")]
impl Power {
    #[zbus(property)]
    fn on_battery(&self) -> bool {
        self.0.lock().unwrap().on_battery
    }
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Device {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        self.0.lock().unwrap().present
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        self.0.lock().unwrap().kind
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.0.lock().unwrap().percentage
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.lock().unwrap().state
    }
    #[zbus(property)]
    fn warning_level(&self) -> u32 {
        self.0.lock().unwrap().warning
    }
}

async fn update(connection: &Connection, shared: &Shared, state: State) -> zbus::Result<()> {
    *shared.lock().unwrap() = state;
    let device = connection
        .object_server()
        .interface::<_, Device>(DISPLAY)
        .await?;
    let device_guard = device.get().await;
    // One notification is sufficient: a correct client re-reads the complete
    // aggregate snapshot, including properties invalidated by the daemon.
    device_guard
        .percentage_changed(device.signal_emitter())
        .await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let socket = PathBuf::from(args.next().ok_or("expected control socket")?);
    if args.next().is_some() {
        return Err("expected exactly one control socket argument".into());
    }
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS")?;
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("no runtime")?);
    if address != format!("unix:path={}", runtime.join("bus").display())
        || socket.parent() != Some(runtime.as_path())
        || !runtime.join("bus.conf").is_file()
        || socket.exists()
    {
        return Err("requires an explicit owned private bus and fresh fixture socket".into());
    }
    let shared = Arc::new(Mutex::new(State {
        present: true,
        kind: 2,
        percentage: 72.0,
        state: 1,
        warning: 1,
        on_battery: false,
    }));
    let connection = Builder::address(address.as_str())?
        .method_timeout(Duration::from_secs(2))
        .name("org.freedesktop.UPower")?
        .serve_at(ROOT, Power(shared.clone()))?
        .serve_at(DISPLAY, Device(shared.clone()))?
        .build()
        .await?;
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut line = String::new();
        BufReader::new(&stream).take(2048).read_line(&mut line)?;
        let result = match serde_json::from_str::<State>(&line) {
            Ok(state) => update(&connection, &shared, state)
                .await
                .map_err(|e| e.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let response = match result {
            Ok(()) => serde_json::json!({"ok": true}),
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        };
        writeln!(stream, "{response}")?;
    }
    Ok(())
}
