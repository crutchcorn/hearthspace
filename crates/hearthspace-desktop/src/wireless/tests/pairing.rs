//! Real AgentManager/Agent1 callback round-trips, with no interactive prompt.
use super::*;
use zbus::message::Header;
const DEVICE: &str = "/org/bluez/hci0/dev_00_11_22_33_44_55";
#[derive(Clone, Default)]
struct Manager {
    registered: Arc<AtomicBool>,
    unregistered: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.bluez.AgentManager1")]
impl Manager {
    fn register_agent(&self, path: OwnedObjectPath, capability: String) {
        assert_eq!(path.as_str(), "/org/hearthspace/WirelessAgent");
        assert_eq!(capability, "KeyboardDisplay");
        self.registered.store(true, Ordering::SeqCst);
    }
    fn unregister_agent(&self, _path: OwnedObjectPath) {
        self.unregistered.store(true, Ordering::SeqCst);
    }
}
struct Device {
    manager: Manager,
    connect: Arc<AtomicUsize>,
    cancel: bool,
    outsider: Connection,
}
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    async fn pair(&self, #[zbus(connection)] c: &Connection, #[zbus(header)] h: Header<'_>) {
        assert!(self.manager.registered.load(Ordering::SeqCst));
        let sender = h.sender().unwrap().as_str();
        // An unrelated bus peer must be rejected before opening a dialog.
        let fake = Proxy::new(
            &self.outsider,
            sender,
            "/org/hearthspace/WirelessAgent",
            "org.bluez.Agent1",
        )
        .await
        .unwrap();
        let result: std::result::Result<String, _> =
            fake.call("RequestPinCode", &(object(DEVICE),)).await;
        assert!(result.is_err());
        if self.cancel {
            let agent = Proxy::new(
                c,
                sender,
                "/org/hearthspace/WirelessAgent",
                "org.bluez.Agent1",
            )
            .await
            .unwrap();
            let _: () = agent.call("Cancel", &()).await.unwrap();
        }
    }
    fn connect(&self) {
        self.connect.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn application_pairing_registers_scoped_agent_and_cancellation_cannot_connect() {
    for cancel in [false, true] {
        let bus = Bus::new();
        let manager = Manager::default();
        let connects = Arc::new(AtomicUsize::new(0));
        let outsider = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let server = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name("org.bluez")
            .unwrap()
            .serve_at("/org/bluez", manager.clone())
            .unwrap()
            .serve_at(
                DEVICE,
                Device {
                    manager: manager.clone(),
                    connect: connects.clone(),
                    cancel,
                    outsider,
                },
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
        let id = format!("{}|{DEVICE}", server.unique_name().unwrap());
        let result = Service::with_connection(client)
            .execute(Command::PairBluetooth(id))
            .await;
        assert_eq!(result.is_err(), cancel);
        assert!(manager.unregistered.load(Ordering::SeqCst));
        assert_eq!(connects.load(Ordering::SeqCst), usize::from(!cancel));
    }
}
