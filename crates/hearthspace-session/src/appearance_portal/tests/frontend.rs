//! Real public portal boundary; no Wayland display or ordinary session bus.
use super::*;
use std::{
    fs,
    process::{Child, Command, Stdio},
};
use zbus::object_server::SignalEmitter;

const GTK: &str = "org.freedesktop.impl.portal.desktop.gtk";
const PUBLIC: &str = "org.freedesktop.portal.Desktop";

struct Gtk;

#[zbus::interface(name = "org.freedesktop.impl.portal.Settings")]
impl Gtk {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    fn read_all(&self, namespaces: Vec<String>) -> Values {
        [
            (
                "org.freedesktop.appearance",
                HashMap::from([("color-scheme".into(), 0u32.into())]),
            ),
            (
                "org.gnome.desktop.interface",
                HashMap::from([
                    (
                        "gtk-theme".into(),
                        zbus::zvariant::Str::from("HostTheme").into(),
                    ),
                    (
                        "font-name".into(),
                        zbus::zvariant::Str::from("Cantarell 11").into(),
                    ),
                ]),
            ),
        ]
        .into_iter()
        .filter(|(namespace, _)| {
            namespaces.is_empty()
                || namespaces
                    .iter()
                    .any(|pattern| settings::namespace_matches(pattern, namespace))
        })
        .map(|(namespace, values)| (namespace.into(), values))
        .collect()
    }

    fn read(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue> {
        self.read_all(vec![])
            .remove(namespace)
            .and_then(|mut values| values.remove(key))
            .ok_or_else(|| zbus::fdo::Error::Failed("unknown setting".into()))
    }

    #[zbus(signal)]
    async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: OwnedValue,
    ) -> zbus::Result<()>;
}

struct Frontend(Child);
impl Drop for Frontend {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn frontend(bus: &Bus) -> Frontend {
    let directory = bus.directory.join("portals");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("hearthspace.portal"),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/xdg-desktop-portal/hearthspace.portal"
        )),
    )
    .unwrap();
    // Only Settings is implemented by this fake GTK backend; no GUI or host
    // D-Bus service may be activated by the actual frontend under test.
    fs::write(
        directory.join("gtk.portal"),
        format!(
            "[portal]\nDBusName={GTK}\nInterfaces={};\nUseIn=Hearthspace\n",
            settings::INTERFACE
        ),
    )
    .unwrap();
    // XDG_DESKTOP_PORTAL_DIR overrides both backend discovery and config lookup
    // in the installed frontend, so its config belongs beside the metadata.
    fs::write(
        directory.join("hearthspace-portals.conf"),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/xdg-desktop-portal/hearthspace-portals.conf"
        )),
    )
    .unwrap();
    let log = bus.directory.join("portal.log");
    let mut frontend = Frontend(
        Command::new("/usr/libexec/xdg-desktop-portal")
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("XDG_RUNTIME_DIR", &bus.directory)
            .env("XDG_CONFIG_HOME", &bus.directory)
            .env("XDG_CONFIG_DIRS", &bus.directory)
            .env("XDG_DATA_HOME", &bus.directory)
            .env("XDG_DATA_DIRS", &bus.directory)
            .env("XDG_CURRENT_DESKTOP", "Hearthspace")
            .env("XDG_DESKTOP_PORTAL_DIR", &directory)
            .env("GSETTINGS_BACKEND", "memory")
            // Only schema definitions come from the system; values stay in
            // memory and no host configuration or activation path is exposed.
            .env("GSETTINGS_SCHEMA_DIR", "/usr/share/glib-2.0/schemas")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("GTK_USE_PORTAL")
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .expect("xdg-desktop-portal is required for the public Settings smoke test"),
    );
    let client = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dbus
        .name_has_owner(PUBLIC.try_into().unwrap())
        .await
        .unwrap()
    {
        assert!(
            frontend.0.try_wait().unwrap().is_none(),
            "portal exited: {}",
            fs::read_to_string(&log).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "portal was not ready: {}",
            fs::read_to_string(&log).unwrap()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    frontend
}

#[tokio::test]
async fn public_portal_reads_live_appearance_preserves_other_gtk_keys_and_filters_host_signals() {
    let bus = Bus::start();
    let daemon = Daemon::start(&bus.directory);
    let _portal = AppearancePortal::start(
        bus.address.clone(),
        daemon.socket.clone(),
        Instant::now() + Duration::from_secs(3),
        &AtomicBool::new(false),
    )
    .unwrap();
    // GTK starts after our worker in the real supervisor.
    let gtk = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .serve_at(settings::PATH, Gtk)
        .unwrap()
        .name(GTK)
        .unwrap()
        .build()
        .await
        .unwrap();
    let _frontend = frontend(&bus).await;
    let client = bus.connection().await;
    let proxy = Proxy::new(
        &client,
        PUBLIC,
        settings::PATH,
        "org.freedesktop.portal.Settings",
    )
    .await
    .unwrap();
    let all: Values = proxy
        .call("ReadAll", &(vec!["org.freedesktop.*", "org.gnome.*"],))
        .await
        .unwrap();
    assert_eq!(
        u32::try_from(&all["org.freedesktop.appearance"]["color-scheme"]).unwrap(),
        2
    );
    assert_eq!(
        <&str>::try_from(&all["org.gnome.desktop.interface"]["gtk-theme"]).unwrap(),
        "Adwaita"
    );
    assert_eq!(
        <&str>::try_from(&all["org.gnome.desktop.interface"]["font-name"]).unwrap(),
        "Cantarell 11"
    );
    let rgb: OwnedValue = proxy
        .call("ReadOne", &("org.freedesktop.appearance", "accent-color"))
        .await
        .unwrap();
    let [r, g, b] = AccentColor::Orange.srgb();
    assert_eq!(<(f64, f64, f64)>::try_from(rgb).unwrap(), (r, g, b));
    let mut signals = proxy.receive_signal("SettingChanged").await.unwrap();
    let emitter = SignalEmitter::new(&gtk, settings::PATH).unwrap();
    Gtk::setting_changed(
        &emitter,
        "org.freedesktop.appearance",
        "color-scheme",
        1u32.into(),
    )
    .await
    .unwrap();
    Gtk::setting_changed(
        &emitter,
        "org.gnome.desktop.interface",
        "gtk-theme",
        zbus::zvariant::Str::from("HostDark").into(),
    )
    .await
    .unwrap();
    Gtk::setting_changed(
        &emitter,
        "org.gnome.desktop.interface",
        "font-name",
        zbus::zvariant::Str::from("Cantarell 13").into(),
    )
    .await
    .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(2), signals.next())
        .await
        .unwrap()
        .unwrap();
    let (namespace, key, value): (String, String, OwnedValue) =
        message.body().deserialize().unwrap();
    assert_eq!(
        (namespace.as_str(), key.as_str()),
        ("org.gnome.desktop.interface", "font-name")
    );
    assert_eq!(String::try_from(value).unwrap(), "Cantarell 13");
    assert!(
        tokio::time::timeout(Duration::from_millis(60), signals.next())
            .await
            .is_err()
    );
    daemon.set(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Blue,
    });
    let mut changed = HashMap::new();
    for _ in 0..4 {
        let message = tokio::time::timeout(Duration::from_secs(2), signals.next())
            .await
            .unwrap()
            .unwrap();
        let (namespace, key, value): (String, String, OwnedValue) =
            message.body().deserialize().unwrap();
        changed.insert((namespace, key), value);
    }
    assert_eq!(
        u32::try_from(
            changed
                .remove(&("org.freedesktop.appearance".into(), "color-scheme".into()))
                .unwrap()
        )
        .unwrap(),
        1
    );
    let rgb = changed
        .remove(&("org.freedesktop.appearance".into(), "accent-color".into()))
        .unwrap();
    let [r, g, b] = AccentColor::Blue.srgb();
    assert_eq!(<(f64, f64, f64)>::try_from(rgb).unwrap(), (r, g, b));
}

#[tokio::test]
async fn public_portal_recovers_changes_missed_during_worker_restart() {
    let bus = Bus::start();
    let daemon = Daemon::start(&bus.directory);
    let mut portal = AppearancePortal::start(
        bus.address.clone(),
        daemon.socket.clone(),
        Instant::now() + Duration::from_secs(3),
        &AtomicBool::new(false),
    )
    .unwrap();
    let _frontend = frontend(&bus).await;
    let client = bus.connection().await;
    let proxy = Proxy::new(
        &client,
        PUBLIC,
        settings::PATH,
        "org.freedesktop.portal.Settings",
    )
    .await
    .unwrap();
    let before: OwnedValue = proxy
        .call("ReadOne", &("org.freedesktop.appearance", "color-scheme"))
        .await
        .unwrap();
    assert_eq!(u32::try_from(before).unwrap(), 2);
    let mut signals = proxy.receive_signal("SettingChanged").await.unwrap();

    // Keep the public portal and its client alive across a backend outage.
    drop(portal.worker.take());
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    assert!(!dbus.name_has_owner(NAME.try_into().unwrap()).await.unwrap());
    daemon.set(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    });
    portal.poll(&AtomicBool::new(false));
    assert!(dbus.name_has_owner(NAME.try_into().unwrap()).await.unwrap());

    let mut recovered = HashMap::new();
    for _ in 0..4 {
        let message = tokio::time::timeout(Duration::from_secs(2), signals.next())
            .await
            .expect("existing clients must receive preferences missed during the outage")
            .unwrap();
        let (namespace, key, value): (String, String, OwnedValue) =
            message.body().deserialize().unwrap();
        let current: OwnedValue = proxy.call("ReadOne", &(&namespace, &key)).await.unwrap();
        assert_eq!(current, value, "reads must agree with the recovered signal");
        recovered.insert((namespace, key), value);
    }
    assert_eq!(recovered.len(), 4);
    assert_eq!(
        u32::try_from(
            recovered
                .remove(&("org.freedesktop.appearance".into(), "color-scheme".into()))
                .unwrap()
        )
        .unwrap(),
        1
    );
    let rgb = recovered
        .remove(&("org.freedesktop.appearance".into(), "accent-color".into()))
        .unwrap();
    let [r, g, b] = AccentColor::Teal.srgb();
    assert_eq!(<(f64, f64, f64)>::try_from(rgb).unwrap(), (r, g, b));
    assert_eq!(
        String::try_from(
            recovered
                .remove(&("org.gnome.desktop.interface".into(), "gtk-theme".into()))
                .unwrap()
        )
        .unwrap(),
        "Adwaita-dark"
    );
    assert_eq!(
        String::try_from(
            recovered
                .remove(&("org.gnome.desktop.interface".into(), "color-scheme".into()))
                .unwrap()
        )
        .unwrap(),
        "prefer-dark"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(350), signals.next())
            .await
            .is_err(),
        "recovery must not turn normal polling into repeated announcements"
    );
}
