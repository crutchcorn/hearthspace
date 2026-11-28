//! Real D-Bus calls over a private bus; never connect to the machine's radios.
use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command as Process, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
struct Bus(Child, String);
impl Bus {
    fn new() -> Self {
        let mut c = Process::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(c.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(c, address.trim().into())
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/1";
const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
fn object(p: &str) -> OwnedObjectPath {
    p.try_into().unwrap()
}
struct Manager {
    settings: Arc<std::sync::Mutex<Option<HashMap<String, Properties>>>>,
    powered: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn add_and_activate_connection(
        &self,
        settings: HashMap<String, Properties>,
        _device: OwnedObjectPath,
        _specific_object: OwnedObjectPath,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        *self.settings.lock().unwrap() = Some(settings);
        (
            object("/org/freedesktop/NetworkManager/Settings/1"),
            object("/org/freedesktop/NetworkManager/ActiveConnection/1"),
        )
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![object(DEVICE)]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.powered.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn set_wireless_enabled(&self, value: bool) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.powered.store(value, Ordering::SeqCst);
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
}
struct Device;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        30
    }
    #[zbus(property)]
    fn available_connections(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
}
struct Wifi(Arc<AtomicUsize>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wifi {
    fn get_access_points(&self) -> Vec<OwnedObjectPath> {
        vec![object(AP)]
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        object("/")
    }
    fn request_scan(&self, _options: Properties) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct AccessPoint(bool);
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        b"Private fixture".to_vec()
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        83
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        u32::from(self.0)
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        0x100
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
}
async fn server(
    bus: &Bus,
    powered: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
    scans: Arc<AtomicUsize>,
) -> Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(
            ROOT,
            Manager {
                powered,
                writes,
                settings: Default::default(),
            },
        )
        .unwrap()
        .serve_at(DEVICE, Device)
        .unwrap()
        .serve_at(DEVICE, Wifi(scans))
        .unwrap()
        .serve_at(AP, AccessPoint(false))
        .unwrap()
        .build()
        .await
        .unwrap()
}
#[tokio::test]
async fn real_bus_snapshot_is_read_only_and_explicit_power_and_scan_reach_only_private_service() {
    let bus = Bus::new();
    let powered = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicUsize::new(0));
    let scans = Arc::new(AtomicUsize::new(0));
    let _server = server(&bus, powered.clone(), writes.clone(), scans.clone()).await;
    let client = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let service = Service::with_connection(client);
    let snapshot = service.snapshot().await;
    let wifi = snapshot.wifi.unwrap();
    assert!(wifi.enabled);
    assert_eq!(wifi.items[0].name, "Private fixture");
    assert_eq!(wifi.items[0].detail, "Open");
    assert_eq!(wifi.items[0].strength, Some(83));
    assert!(snapshot.bluetooth.is_err());
    assert!(
        service
            .execute(Command::DisconnectWifi(wifi.items[0].id.clone()))
            .await
            .is_err()
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(scans.load(Ordering::SeqCst), 0);
    service.execute(Command::WifiPower(false)).await.unwrap();
    assert!(!powered.load(Ordering::SeqCst));
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    service.execute(Command::ScanWifi).await.unwrap();
    assert_eq!(scans.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn old_list_entry_cannot_activate_a_replacement_service() {
    let bus = Bus::new();
    let powered = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicUsize::new(0));
    let scans = Arc::new(AtomicUsize::new(0));
    let server = server(&bus, powered, writes, scans).await;
    let client = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let service = Service::with_connection(client);
    let id = service.snapshot().await.wifi.unwrap().items.remove(0).id;
    server
        .release_name("org.freedesktop.NetworkManager")
        .await
        .unwrap();
    let new_power = Arc::new(AtomicBool::new(true));
    let new_writes = Arc::new(AtomicUsize::new(0));
    let _replacement = self::server(
        &bus,
        new_power,
        new_writes.clone(),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let result = service.execute(Command::ConnectWifi(id)).await;
    assert!(result.unwrap_err().contains("restarted"));
    assert_eq!(new_writes.load(Ordering::SeqCst), 0);
}
#[test]
fn commands_require_an_explicit_live_sender_and_never_optimistically_change_radio() {
    let mut state = state::State::default();
    state.request(Command::WifiPower(false));
    assert!(!state.busy);
    assert!(state.error.is_some());
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    state.sender = Some(tx);
    state.request(Command::WifiPower(false));
    assert!(state.busy);
    assert!(matches!(rx.try_recv(), Ok(Command::WifiPower(false))));
    state.request(Command::BluetoothPower(false));
    assert!(rx.try_recv().is_err());
    assert!(state.snapshot.wifi.is_err());
}
struct Adapter(Arc<AtomicBool>);
#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn set_powered(&self, v: bool) {
        self.0.store(v, Ordering::SeqCst);
    }
}
struct BluetoothDevice(Arc<AtomicBool>);
#[zbus::interface(name = "org.bluez.Device1")]
impl BluetoothDevice {
    #[zbus(property)]
    fn alias(&self) -> &str {
        "Fixture headphones"
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    fn connect(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    fn disconnect(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn bluez_objects_and_explicit_device_actions_use_private_bus() {
    let bus = Bus::new();
    let power = Arc::new(AtomicBool::new(true));
    let connected = Arc::new(AtomicBool::new(false));
    let _server = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at("/org/bluez/hci0", Adapter(power.clone()))
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF",
            BluetoothDevice(connected.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let service = Service::with_connection(client);
    let radio = service.snapshot().await.bluetooth.unwrap();
    assert!(radio.enabled);
    assert!(!radio.items[0].connected);
    assert!(radio.items[0].paired);
    let id = radio.items[0].id.clone();
    service
        .execute(Command::ConnectBluetooth(id.clone()))
        .await
        .unwrap();
    assert!(connected.load(Ordering::SeqCst));
    assert!(service.snapshot().await.bluetooth.unwrap().items[0].connected);
    service
        .execute(Command::DisconnectBluetooth(id))
        .await
        .unwrap();
    assert!(!connected.load(Ordering::SeqCst));
    service
        .execute(Command::BluetoothPower(false))
        .await
        .unwrap();
    assert!(!power.load(Ordering::SeqCst));
}

struct Active;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
}
#[tokio::test]
async fn secured_connection_sends_secret_only_in_standard_nm_settings_and_cancel_never_activates() {
    let bus = Bus::new();
    let settings = Arc::new(std::sync::Mutex::new(None));
    let server = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(
            ROOT,
            Manager {
                powered: Arc::new(AtomicBool::new(true)),
                writes: Arc::new(AtomicUsize::new(0)),
                settings: settings.clone(),
            },
        )
        .unwrap()
        .serve_at(DEVICE, Device)
        .unwrap()
        .serve_at(DEVICE, Wifi(Arc::new(AtomicUsize::new(0))))
        .unwrap()
        .serve_at(AP, AccessPoint(true))
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager/ActiveConnection/1", Active)
        .unwrap()
        .build()
        .await
        .unwrap();
    let c = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let service = Service::with_connection(c.clone());
    let id = service.snapshot().await.wifi.unwrap().items.remove(0).id;
    assert!(
        wifi::connect_with_password(&c, &id, || async { Err("Cancelled".into()) })
            .await
            .is_err()
    );
    assert!(settings.lock().unwrap().is_none());
    wifi::connect_with_password(&c, &id, || async { Ok("fixture-password".into()) })
        .await
        .unwrap();
    let captured = settings.lock().unwrap().take().unwrap();
    let security = &captured["802-11-wireless-security"];
    assert_eq!(<&str>::try_from(&security["key-mgmt"]).unwrap(), "wpa-psk");
    assert_eq!(
        <&str>::try_from(&security["psk"]).unwrap(),
        "fixture-password"
    );
    let result = wifi::connect_with_password(&c, &id, || async {
        server
            .release_name("org.freedesktop.NetworkManager")
            .await
            .unwrap();
        Ok("fixture-password".into())
    })
    .await;
    assert!(
        result.is_err(),
        "daemon loss during password entry must not create a profile"
    );
    assert!(settings.lock().unwrap().is_none());
}

mod disconnect;
mod pairing;
