use super::{BatteryState, test_support::*};

#[tokio::test]
async fn installed_idle_daemon_is_activated_on_the_private_bus() {
    let bus = Bus::activatable();
    let mut monitor = Monitor::start(&bus, None);
    let status = monitor.next(|status| status.is_some()).await.unwrap();
    assert_eq!(status.percentage, 72);
    monitor.stop().await;
}

// The isolated bus activates this test executable as its service. Normal test
// runs return immediately; the child has an absolute lifetime bound and exits
// as soon as its private bus closes. No host service or environment is changed.
#[tokio::test]
async fn activation_service_process() {
    if std::env::var("HEARTHSPACE_TEST_BATTERY_ACTIVATION").as_deref() != Ok("1") {
        return;
    }
    let address = std::env::var("DBUS_STARTER_ADDRESS").unwrap();
    let connection = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .method_timeout(std::time::Duration::from_millis(500))
        .build()
        .await
        .unwrap();
    let power = Power::on_connection(connection, State::default()).await;
    let bus = zbus::fdo::DBusProxy::new(&power.connection).await.unwrap();
    let lifetime = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(lifetime);
    loop {
        tokio::select! {
            _ = &mut lifetime => break,
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                if bus.list_names().await.is_err() { break; }
            }
        }
    }
    // This is only the activation subprocess; the parent may already have
    // closed the inherited output pipe along with its private bus.
    std::process::exit(0);
}

#[tokio::test]
async fn missing_daemon_hotplug_and_property_invalidation_recover() {
    let bus = Bus::start();
    let mut monitor = Monitor::start(&bus, None);
    assert!(monitor.next(|status| status.is_none()).await.is_none());
    let power = Power::start(
        &bus,
        State {
            present: false,
            ..State::default()
        },
    )
    .await;
    power.change(|state| state.present = true).await;
    let status = monitor.next(|status| status.is_some()).await.unwrap();
    assert_eq!(status.percentage, 72);
    assert!(status.is_charging());
    power
        .change(|state| {
            state.on_battery = true;
            state.state = 2;
            state.percentage = 50.0;
        })
        .await;
    let status = monitor
        .next(|status| status.is_some_and(|value| value.percentage == 50))
        .await
        .unwrap();
    assert_eq!(status.state, BatteryState::Discharging);
    assert!(status.on_battery);
    power.change(|state| state.present = false).await;
    assert!(monitor.next(|status| status.is_none()).await.is_none());
    power.change(|state| state.present = true).await;
    monitor.next(|status| status.is_some()).await;
    monitor.stop().await;
}

#[tokio::test]
async fn invalid_data_and_daemon_restart_clear_old_readings() {
    let bus = Bus::start();
    let power = Power::start(&bus, State::default()).await;
    let mut monitor = Monitor::start(&bus, None);
    monitor.next(|status| status.is_some()).await;
    power.change(|state| state.percentage = f64::NAN).await;
    monitor.next(|status| status.is_none()).await;
    power.change(|state| state.percentage = 90.0).await;
    monitor
        .next(|status| status.is_some_and(|status| status.percentage == 90))
        .await;
    power.stop().await;
    monitor.next(|status| status.is_none()).await;
    let _replacement = Power::start(
        &bus,
        State {
            percentage: 61.0,
            ..State::default()
        },
    )
    .await;
    monitor
        .next(|status| status.is_some_and(|status| status.percentage == 61))
        .await;
    monitor.stop().await;
}

#[tokio::test]
async fn stalled_daemon_clears_stale_data_within_the_read_deadline() {
    let bus = Bus::start();
    let power = Power::start(&bus, State::default()).await;
    let mut monitor = Monitor::start(&bus, None);
    monitor.next(|status| status.is_some()).await;
    power.change(|state| state.stalled = true).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        monitor.next(|status| status.is_none()),
    )
    .await
    .unwrap();
    power
        .change(|state| {
            state.stalled = false;
            state.percentage = 63.0;
        })
        .await;
    monitor
        .next(|status| status.is_some_and(|status| status.percentage == 63))
        .await;
    monitor.stop().await;
    power.stop().await;
}

#[tokio::test]
async fn private_notifications_deduplicate_across_updates_and_upower_restart() {
    let bus = Bus::start();
    let private_bus = Bus::start();
    let (_notifications, mut notices) = notifications(&private_bus).await;
    let power = Power::start(&bus, State::default()).await;
    let mut monitor = Monitor::start(&bus, Some(&private_bus));
    monitor.next(|status| status.is_some()).await;
    power
        .change(|state| {
            state.on_battery = true;
            state.state = 2;
            state.percentage = 14.0;
            state.warning = 3;
        })
        .await;
    let low = notice(&mut notices).await;
    assert_eq!(low.summary, "Low battery");
    assert_eq!(low.urgency, 1);
    power.change(|state| state.percentage = 13.0).await;
    monitor
        .next(|status| status.is_some_and(|status| status.percentage == 13))
        .await;
    let state = power.state.lock().unwrap().clone();
    power.stop().await;
    monitor.next(|status| status.is_none()).await;
    let power = Power::start(&bus, state).await;
    monitor
        .next(|status| status.is_some_and(|status| status.percentage == 13))
        .await;
    power
        .change(|state| {
            state.percentage = 4.0;
            state.warning = 4;
        })
        .await;
    let critical = notice(&mut notices).await;
    assert_eq!(critical.summary, "Critical battery");
    assert_eq!(critical.urgency, 2);
    assert!(notices.try_recv().is_err());
    // AC suppresses stale critical data and rearms the next discharge episode.
    power
        .change(|state| {
            state.on_battery = false;
            state.state = 1;
        })
        .await;
    monitor
        .next(|status| status.is_some_and(|status| status.is_charging()))
        .await;
    power
        .change(|state| {
            state.on_battery = true;
            state.state = 2;
        })
        .await;
    assert_eq!(notice(&mut notices).await.urgency, 2);
    monitor.stop().await;
}

#[tokio::test]
async fn no_notification_address_never_uses_the_system_bus_notification_service() {
    let bus = Bus::start();
    let (_notifications, mut notices) = notifications(&bus).await;
    let _power = Power::start(
        &bus,
        State {
            percentage: 2.0,
            state: 2,
            warning: 4,
            on_battery: true,
            ..State::default()
        },
    )
    .await;
    let mut monitor = Monitor::start(&bus, None);
    monitor.next(|status| status.is_some()).await;
    assert!(notices.try_recv().is_err());
    monitor.stop().await;
}
