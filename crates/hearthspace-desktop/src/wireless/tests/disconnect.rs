use super::*;
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/7";
struct Manager(Arc<AtomicUsize>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![object(DEVICE)]
    }
    fn deactivate_connection(&self, active: OwnedObjectPath) {
        assert_eq!(active.as_str(), ACTIVE);
        self.0.fetch_add(1, Ordering::SeqCst);
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
    fn active_connection(&self) -> OwnedObjectPath {
        object(ACTIVE)
    }
}
struct Wifi;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wifi {
    fn get_access_points(&self) -> Vec<OwnedObjectPath> {
        vec![object(AP)]
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        object(AP)
    }
}
struct Active(bool);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn specific_object(&self) -> OwnedObjectPath {
        object(if self.0 {
            AP
        } else {
            "/org/freedesktop/NetworkManager/AccessPoint/2"
        })
    }
}
#[tokio::test]
async fn disconnect_targets_exact_active_connection_and_refuses_replacement_network() {
    for matches in [false, true] {
        let bus = Bus::new();
        let writes = Arc::new(AtomicUsize::new(0));
        let server = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name("org.freedesktop.NetworkManager")
            .unwrap()
            .serve_at(ROOT, Manager(writes.clone()))
            .unwrap()
            .serve_at(DEVICE, Device)
            .unwrap()
            .serve_at(DEVICE, Wifi)
            .unwrap()
            .serve_at(ACTIVE, Active(matches))
            .unwrap()
            .build()
            .await
            .unwrap();
        let c = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let id = format!("{}|{AP}", server.unique_name().unwrap());
        let result = Service::with_connection(c)
            .execute(Command::DisconnectWifi(id))
            .await;
        assert_eq!(result.is_ok(), matches);
        assert_eq!(writes.load(Ordering::SeqCst), usize::from(matches));
    }
}
