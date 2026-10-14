//! Real GTK client for window-state, transient-dialog, popup and input tests.
use gtk::{gdk, gio, glib, prelude::*};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args().nth(1).ok_or("snapshot path required")?);
    let app = gtk::Application::builder()
        .application_id("dev.hearthspace.WindowTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| build(app, path.clone()));
    app.run_with_args(&["hearthspace-window-test"]);
    Ok(())
}

fn build(app: &gtk::Application, path: PathBuf) {
    let size = std::env::var("HEARTHSPACE_WINDOW_TEST_SIZE")
        .ok()
        .map(|value| {
            let (width, height) = value.split_once('x').expect("test size WIDTHxHEIGHT");
            (
                width.parse::<i32>().unwrap(),
                height.parse::<i32>().unwrap(),
            )
        })
        .unwrap_or((560, 360));
    let css = gtk::CssProvider::new();
    css.load_from_data(".fixture-editor, .fixture-editor text {background: #00cc66; color: #000000;} .fixture-popup {background: #dc143c; color: #ffffff; padding: 20px;}");
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Window states fixture")
        .default_width(size.0)
        .default_height(size.1)
        .build();
    let editor = gtk::TextView::builder().monospace(true).build();
    editor.add_css_class("fixture-editor");
    window.set_child(Some(&editor));
    let dialog = Rc::new(RefCell::new(None::<gtk::Window>));
    let popover = gtk::Popover::new();
    popover.set_parent(&editor);
    let dismiss = gtk::Button::with_label("Dismiss popup");
    dismiss.set_size_request(240, 160);
    dismiss.add_css_class("fixture-popup");
    popover.set_child(Some(&dismiss));
    dismiss.connect_clicked(glib::clone!(
        #[weak]
        popover,
        move |_| popover.popdown()
    ));
    let last_key = Rc::new(RefCell::new(String::new()));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        window,
        #[weak]
        editor,
        #[weak]
        popover,
        #[strong]
        dialog,
        #[strong]
        last_key,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, _| {
            *last_key.borrow_mut() = format!("{key:?}");
            match key {
                gdk::Key::F11 => {
                    if window.is_fullscreen() {
                        window.unfullscreen();
                    } else {
                        window.fullscreen();
                    }
                }
                gdk::Key::F9 => {
                    if window.is_maximized() {
                        window.unmaximize();
                    } else {
                        window.maximize();
                    }
                }
                gdk::Key::F6 | gdk::Key::F8 => {
                    let size = if key == gdk::Key::F8 {
                        (1600, 1200)
                    } else {
                        (300, 180)
                    };
                    let child = gtk::Window::builder()
                        .title("Window fixture dialog")
                        .transient_for(&window)
                        .modal(true)
                        .default_width(size.0)
                        .default_height(size.1)
                        .build();
                    child.set_child(Some(
                        &gtk::Entry::builder()
                            .placeholder_text("Dialog text")
                            .build(),
                    ));
                    *dialog.borrow_mut() = Some(child.clone());
                    child.present();
                }
                gdk::Key::F7 => {
                    popover.set_pointing_to(Some(&gdk::Rectangle::new(
                        editor.width() - 2,
                        editor.height() - 2,
                        1,
                        1,
                    )));
                    popover.popup();
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    ));
    window.add_controller(keys);
    glib::timeout_add_local(
        std::time::Duration::from_millis(25),
        glib::clone!(
            #[weak]
            window,
            #[weak]
            editor,
            #[weak]
            popover,
            #[strong]
            dialog,
            #[strong]
            last_key,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                let buffer = editor.buffer();
                let state = serde_json::json!({
                    "active": window.is_active(), "fullscreen": window.is_fullscreen(), "maximized": window.is_maximized(),
                    "width": window.width(), "height": window.height(),
                    "text": buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).as_str(),
                    "popup": popover.is_mapped(), "popup_visible": popover.is_visible(),
                    "popup_width": popover.width(), "popup_height": popover.height(),
                    "last_key": last_key.borrow().as_str(),
                    "dialog": dialog.borrow().as_ref().is_some_and(|window| window.is_mapped()),
                });
                let temporary = path.with_extension("json.tmp");
                std::fs::write(&temporary, serde_json::to_vec(&state).unwrap()).unwrap();
                std::fs::rename(temporary, &path).unwrap();
                glib::ControlFlow::Continue
            }
        ),
    );
    window.present();
    editor.grab_focus();
}
