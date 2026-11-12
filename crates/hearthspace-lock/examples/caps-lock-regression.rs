//! Production widgets in an ordinary GTK window: no authentication or lock role.
//! Pass a fresh report path, send Caps Lock through the owned test display,
//! then create REPORT.finish after at least three transitions to finish.
#[path = "../src/background.rs"]
mod background;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/ui.rs"]
mod ui;

use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("fresh report path required"));
    assert!(
        path.is_absolute(),
        "use an absolute path in the owned evidence directory"
    );
    let finish = path.with_extension("finish");
    assert!(!finish.exists(), "finish marker must be fresh");
    let report = Rc::new(RefCell::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap(),
    ));
    gtk::init().expect("an explicitly owned GTK display is required");
    let display = gdk::Display::default().unwrap();
    let monitor = display
        .monitors()
        .item(0)
        .unwrap()
        .downcast::<gdk::Monitor>()
        .unwrap();
    let keyboard = display.default_seat().unwrap().keyboard().unwrap();
    let mut config = config::Config::load();
    config.settings.background_image_path = None;
    config.display_name = "Caps Lock state regression".into();
    let screen = ui::Screen::new(&config, &monitor, |_| {
        panic!("fixture must never submit a password")
    });
    screen.clear_secret();
    screen.set_busy(false);
    screen.set_feedback("");
    screen.window.set_title(Some("Caps Lock state regression"));
    screen.window.set_default_size(900, 760);
    screen.window.present();
    let entry = find(screen.window.upcast_ref(), &|w| {
        w.is::<gtk::PasswordEntry>()
    })
    .unwrap()
    .downcast::<gtk::PasswordEntry>()
    .unwrap();
    let text = entry.delegate().unwrap().downcast::<gtk::Text>().unwrap();
    let icon = find(entry.upcast_ref(), &|w| {
        w.has_css_class("caps-lock-indicator")
    })
    .unwrap();
    let feedback = find(screen.window.upcast_ref(), &|w| w.has_css_class("feedback"))
        .unwrap()
        .downcast::<gtk::Label>()
        .unwrap();
    let changes = Rc::new(Cell::new(0));
    let last = Rc::new(Cell::new(keyboard.is_caps_locked()));
    let changes_complete = changes.clone();
    let weak_keyboard = keyboard.downgrade();
    let sample = Rc::new(move |phase: &str| {
        if !text.has_focus() || text.is_visible() {
            return false;
        }
        let caps = weak_keyboard.upgrade().unwrap().is_caps_locked();
        if caps != last.replace(caps) {
            changes.set(changes.get() + 1);
        }
        let warning = feedback.label() == "Caps Lock is on.";
        let indicator = icon.is_visible();
        writeln!(
            report.borrow_mut(),
            "{phase} {} {caps} {warning} {indicator}",
            changes.get()
        )
        .unwrap();
        report.borrow_mut().flush().unwrap();
        assert_eq!(
            warning, caps,
            "text must match the keyboard before another key event"
        );
        assert_eq!(
            indicator, caps,
            "fixture must observe GTK's actual indicator"
        );
        true
    });
    let observe = sample.clone();
    let notification = keyboard.connect_caps_lock_state_notify(move |_| {
        observe("notify");
    });
    let main_loop = glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let start = Instant::now();
    let ready = Cell::new(false);
    let baseline = Cell::new(0);
    let check = sample.clone();
    glib::timeout_add_local(Duration::from_millis(20), move || {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "fixture deadline expired"
        );
        if !ready.get() && check("ready") {
            baseline.set(changes_complete.get());
            ready.set(true);
        }
        if finish.exists() {
            assert!(ready.get());
            check("done");
            assert!(
                changes_complete.get() >= baseline.get() + 3,
                "send at least three Caps Lock transitions after ready"
            );
            quit.quit();
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    main_loop.run();
    keyboard.disconnect(notification);
    screen.window.destroy();
    println!("Caps Lock text and GTK indicator remained synchronized");
}

fn find(root: &gtk::Widget, matches: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if matches(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find(&widget, matches) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}
