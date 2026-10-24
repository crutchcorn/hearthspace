//! Real GTK widget regression; run on an available display, without acquiring a lock:
//! `cargo run -p hearthspace-lock --example secret-entry-regression`
//! This fixture never authenticates or acquires a session lock.

#[path = "../src/background.rs"]
mod background;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/ui.rs"]
mod ui;

use std::{cell::RefCell, rc::Rc};

use gtk::{gdk, prelude::*};
use zeroize::Zeroizing;

fn main() {
    gtk::init().expect("run the widget regression on an available GTK display");
    let monitor = gdk::Display::default()
        .expect("GTK display")
        .monitors()
        .item(0)
        .expect("display must have a monitor")
        .downcast::<gdk::Monitor>()
        .expect("GdkMonitor");
    let mut config = config::Config::load();
    config.settings.background_image_path = None;
    config.display_name = "Secret entry regression".into();
    let received: Rc<RefCell<Option<Zeroizing<String>>>> = Rc::default();
    let sink = Rc::clone(&received);
    let screen = ui::Screen::new(&config, &monitor, move |secret| {
        assert!(sink.borrow().is_none(), "submission must occur only once");
        *sink.borrow_mut() = Some(secret);
    });
    let entry = find(screen.window.upcast_ref(), &|widget| {
        widget.is::<gtk::PasswordEntry>()
    })
    .expect("production password entry")
    .downcast::<gtk::PasswordEntry>()
    .unwrap();
    let submit = find(screen.window.upcast_ref(), &|widget| {
        widget.is::<gtk::Button>() && widget.has_css_class("submit")
    })
    .expect("production submit button")
    .downcast::<gtk::Button>()
    .unwrap();
    let text = entry.delegate().unwrap().downcast::<gtk::Text>().unwrap();
    let eye = find(screen.window.upcast_ref(), &|widget| {
        widget.is::<gtk::Button>() && widget.has_css_class("reveal")
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap();

    assert!(
        !screen.window.is_realized(),
        "Screen must not realize its window"
    );
    assert!(!submit.gets_focus_on_click());
    assert!(!submit.is_sensitive());
    assert_masked_empty(&text);

    eye.emit_clicked();
    assert!(text.is_visible());
    assert_eq!(eye.tooltip_text().as_deref(), Some("Hide password"));
    screen.clear_secret();
    assert_masked_empty(&text);
    assert_eq!(eye.tooltip_text().as_deref(), Some("Show password"));

    // The same public clear operation is used for focus/output changes and errors.
    entry.set_text("synthetic first attempt");
    reveal(&text);
    screen.clear_secret();
    assert_masked_empty(&text);
    entry.set_text("synthetic second attempt");
    assert!(!text.is_visible(), "a new attempt must start masked");
    screen.clear_secret();

    // Reveal is persistent even when empty; clearing an empty field must reset it.
    reveal(&text);
    screen.clear_secret();
    assert_masked_empty(&text);

    // Enter exercises the real UI extraction callback, including Unicode copying.
    entry.set_text("synthetic p\u{1f512}ssword");
    assert!(submit.is_sensitive());
    reveal(&text);
    entry.emit_by_name::<()>("activate", &[]);
    assert_masked_empty(&text);
    assert!(!submit.is_sensitive());
    assert_received(&received, "synthetic p\u{1f512}ssword");

    screen.clear_secret();
    screen.set_feedback("Synthetic authentication rejection");
    screen.set_busy(false);
    entry.set_text("synthetic click attempt");
    assert!(
        !text.is_visible(),
        "retry after rejection must remain masked"
    );
    reveal(&text);
    submit.emit_clicked();
    assert_masked_empty(&text);
    assert_received(&received, "synthetic click attempt");

    // Empty or disabled forms cannot produce an authentication request.
    entry.emit_by_name::<()>("activate", &[]);
    assert!(received.borrow().is_none());
    entry.set_text("synthetic busy attempt");
    screen.set_busy(true);
    entry.emit_by_name::<()>("activate", &[]);
    assert!(received.borrow().is_none());
    screen.clear_secret();
    assert_masked_empty(&text);
    screen.window.destroy();
    println!("Secret entry regression passed: clears/remasks, Enter/click, Unicode, busy/empty.");
}

fn reveal(text: &gtk::Text) {
    text.activate_action("misc.toggle-visibility", None)
        .expect("GTK's real reveal action");
    assert!(
        text.is_visible(),
        "fixture must actually reveal the password"
    );
}

fn assert_masked_empty(text: &gtk::Text) {
    assert_eq!(text.text_length(), 0);
    assert!(!text.is_visible(), "clear must reset password masking");
}

fn assert_received(received: &RefCell<Option<Zeroizing<String>>>, expected: &str) {
    let secret = received.borrow_mut().take().expect("one submission");
    // Never print the captured value, even in a failed assertion.
    assert!(
        secret.as_str() == expected,
        "submitted bytes must match input"
    );
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
