use super::*;
use futures_util::StreamExt;
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use std::collections::HashMap;
use zbus::{Connection, Proxy, zvariant::OwnedValue};

#[path = "tests/frontend.rs"]
mod frontend;
#[path = "tests/support.rs"]
mod support;
use support::{Bus, Daemon};

type Values = HashMap<String, HashMap<String, OwnedValue>>;

async fn proxy(connection: &Connection) -> Proxy<'_> {
    Proxy::new(connection, NAME, settings::PATH, settings::INTERFACE)
        .await
        .unwrap()
}

async fn read(proxy: &Proxy<'_>, namespace: &str, key: &str) -> OwnedValue {
    proxy.call("Read", &(namespace, key)).await.unwrap()
}

#[tokio::test]
async fn backend_methods_filter_namespaces_and_report_wire_types() {
    let bus = Bus::start();
    let state = Settings::new(Appearance::default());
    let _server = settings::connect(&bus.address, state).await.unwrap();
    let client = bus.connection().await;
    let proxy = proxy(&client).await;
    assert_eq!(proxy.get_property::<u32>("version").await.unwrap(), 1);
    let all: Values = proxy
        .call("ReadAll", &(Vec::<String>::new(),))
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    let standard: Values = proxy
        .call("ReadAll", &(vec!["org.freedesktop.*"],))
        .await
        .unwrap();
    assert_eq!(standard.len(), 1);
    assert_eq!(
        u32::try_from(&standard["org.freedesktop.appearance"]["color-scheme"]).unwrap(),
        2
    );
    let rgb = <(f64, f64, f64)>::try_from(
        read(&proxy, "org.freedesktop.appearance", "accent-color").await,
    )
    .unwrap();
    let [r, g, b] = AccentColor::Orange.srgb();
    assert_eq!(rgb, (r, g, b));
    assert_eq!(
        String::try_from(read(&proxy, "org.gnome.desktop.interface", "gtk-theme").await).unwrap(),
        "Adwaita"
    );
    let missing: Values = proxy
        .call("ReadAll", &(vec!["not.present", "org.*.appearance"],))
        .await
        .unwrap();
    assert!(missing.is_empty());
    let unknown = proxy
        .call::<_, _, OwnedValue>("Read", &("unknown", "key"))
        .await
        .unwrap_err();
    assert!(
        unknown
            .to_string()
            .contains("org.freedesktop.portal.Error.NotFound")
    );
    assert!(
        settings::connect(&bus.address, Settings::new(Appearance::default()))
            .await
            .is_err(),
        "another publisher must not take over the session's appearance"
    );
}

#[tokio::test]
async fn backend_publishes_only_changed_values_and_updates_reads_before_signals() {
    let bus = Bus::start();
    let state = Settings::new(Appearance::default());
    let server = settings::connect(&bus.address, state.clone())
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = proxy(&client).await;
    let mut signals = proxy.receive_signal("SettingChanged").await.unwrap();
    let appearance = Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    };
    state.publish(&server, appearance).await.unwrap();
    let mut changes = HashMap::new();
    for _ in 0..4 {
        let message = tokio::time::timeout(Duration::from_secs(2), signals.next())
            .await
            .unwrap()
            .unwrap();
        let (namespace, key, value): (String, String, OwnedValue) =
            message.body().deserialize().unwrap();
        assert_eq!(read(&proxy, &namespace, &key).await, value);
        changes.insert((namespace, key), value);
    }
    assert_eq!(changes.len(), 4);
    assert_eq!(
        String::try_from(
            changes
                .remove(&("org.gnome.desktop.interface".into(), "gtk-theme".into()))
                .unwrap()
        )
        .unwrap(),
        "Adwaita-dark"
    );
    state.publish(&server, appearance).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), signals.next())
            .await
            .is_err(),
        "unchanged preferences must not generate periodic notifications"
    );
}

#[tokio::test]
async fn worker_tracks_the_explicit_daemon_and_releases_its_bus_name_on_shutdown() {
    let bus = Bus::start();
    let daemon = Daemon::start(&bus.directory);
    let portal = AppearancePortal::start(
        bus.address.clone(),
        daemon.socket.clone(),
        Instant::now() + Duration::from_secs(3),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = bus.connection().await;
    let proxy = proxy(&client).await;
    let mut signals = proxy.receive_signal("SettingChanged").await.unwrap();
    daemon.set(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Orange,
    });
    let changed = tokio::time::timeout(Duration::from_secs(2), signals.next())
        .await
        .unwrap()
        .unwrap();
    let (_, _, _): (String, String, OwnedValue) = changed.body().deserialize().unwrap();
    assert_eq!(
        u32::try_from(read(&proxy, "org.freedesktop.appearance", "color-scheme").await).unwrap(),
        1
    );
    // Losing settingsd must not revert already published preferences.
    drop(daemon);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        u32::try_from(read(&proxy, "org.freedesktop.appearance", "color-scheme").await).unwrap(),
        1
    );
    drop(portal);
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    assert!(!dbus.name_has_owner(NAME.try_into().unwrap()).await.unwrap());
}
