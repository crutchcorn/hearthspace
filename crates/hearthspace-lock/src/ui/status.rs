use super::elements;
use gtk::{glib, prelude::*};
use hearthspace_ui_components::{Palette, icons::Icon};
use std::time::Duration;

pub(super) fn header(palette: Palette) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    let lock = elements::icon(Icon::Lock, 17, palette.accent_decorative);
    lock.set_tooltip_text(Some("Session locked"));
    header.append(&lock);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let status = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    status.add_css_class("status");
    let network = gtk::gio::NetworkMonitor::default();
    let wifi = elements::icon(
        if network.is_network_available() {
            Icon::Wifi
        } else {
            Icon::WifiOff
        },
        18,
        palette.muted,
    );
    wifi.set_tooltip_text(Some(if network.is_network_available() {
        "Network available"
    } else {
        "Network unavailable"
    }));
    status.append(&wifi);
    let weak_wifi = wifi.downgrade();
    let network_changed = network.connect_network_changed(move |_, available| {
        if let Some(wifi) = weak_wifi.upgrade() {
            let image = elements::icon(
                if available { Icon::Wifi } else { Icon::WifiOff },
                18,
                palette.muted,
            );
            wifi.set_paintable(image.paintable().as_ref());
            wifi.set_tooltip_text(Some(if available {
                "Network available"
            } else {
                "Network unavailable"
            }));
        }
    });
    let network_changed = std::cell::RefCell::new(Some(network_changed));
    header.connect_destroy(move |_| {
        if let Some(handler) = network_changed.borrow_mut().take() {
            network.disconnect(handler);
        }
    });
    let battery = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    battery.set_visible(false);
    let battery_icon = elements::icon(Icon::BatteryFull, 20, palette.muted);
    let percentage = gtk::Label::new(None);
    battery.append(&battery_icon);
    battery.append(&percentage);
    status.append(&battery);
    header.append(&status);
    let (send, mut updates) = tokio::sync::mpsc::unbounded_channel();
    let _ = std::thread::Builder::new()
        .name("lock-battery".into())
        .spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(hearthspace_shell::battery::monitor(send, None));
            }
        });
    let weak = battery.downgrade();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let Some(battery) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        while let Ok(value) = updates.try_recv() {
            battery.set_visible(value.is_some());
            if let Some(value) = value {
                percentage.set_label(&value.label());
                battery.set_tooltip_text(Some(&value.accessible_label()));
            }
        }
        glib::ControlFlow::Continue
    });
    header
}
