//! A normal, non-locking window for visual checks of the production widgets.
//! This executable has no PAM, lock protocol, or unlock capability.
#[path = "../src/background.rs"]
mod background;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/ui.rs"]
mod ui;

use gtk::{gio, glib, prelude::*};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.hearthspace.LockPreview")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        let monitor = gtk::gdk::Display::default()
            .unwrap()
            .monitors()
            .item(0)
            .unwrap()
            .downcast::<gtk::gdk::Monitor>()
            .unwrap();
        let mut config = config::Config::load();
        if std::env::args().any(|value| value == "--dark") {
            config.settings.appearance.color_mode = hearthspace_common::ColorMode::Dark;
        }
        let holder: Rc<RefCell<Weak<ui::Screen>>> = Default::default();
        let target = holder.clone();
        let screen = Rc::new(ui::Screen::new(&config, &monitor, move |password| {
            drop(password);
            if let Some(screen) = target.borrow().upgrade() {
                screen.clear_secret();
                screen.set_feedback("That password didn’t match. Try again.");
                screen.set_busy(false);
            }
        }));
        *holder.borrow_mut() = Rc::downgrade(&screen);
        screen.window.set_application(Some(app));
        screen.window.set_default_size(1280, 800);
        screen.window.set_title(Some("Lock screen visual preview"));
        screen.window.present();
        // Keep only the presentation alive for this standalone fixture.
        app.connect_shutdown(move |_| screen.clear_secret());
    });
    app.run_with_args(&["lock-preview"])
}
