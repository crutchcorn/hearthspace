//! Real GTK client for window-state, transient-dialog, popup and input tests.
use gtk::{gdk, gio, glib, prelude::*};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args().nth(1).ok_or("snapshot path required")?);
    let shell_bar = std::env::var_os("HEARTHSPACE_WINDOW_TEST_SHELL_BAR").is_some();
    let app_id = if shell_bar {
        hearthspace_compositor::config::SHELL_BAR_APP_ID
    } else {
        "dev.hearthspace.WindowTest"
    };
    // GTK 4.14 falls back to the program name for xdg_toplevel.app_id when
    // no session bus is available, as on CI. Keep that identity explicit too.
    glib::set_prgname(Some(app_id));
    let app = gtk::Application::builder()
        .application_id(app_id)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        if shell_bar {
            build_shell_bar(app, path.clone());
        } else {
            build(app, path.clone());
        }
    });
    app.run_with_args(&["hearthspace-window-test"]);
    Ok(())
}

fn build_shell_bar(app: &gtk::Application, path: PathBuf) {
    use std::{
        cell::Cell,
        io::{Read, Write},
        os::unix::net::UnixStream,
    };

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Canvas zoom control fixture")
        .decorated(false)
        .default_width(1280)
        .default_height(48)
        .build();
    let button = gtk::Button::with_label("Zoom out");
    let clicked = Rc::new(Cell::new(false));
    button.connect_clicked(glib::clone!(
        #[strong]
        clicked,
        move |_| {
            let socket = std::env::var_os("HEARTHSPACE_COMMAND_SOCKET").unwrap();
            let mut stream = UnixStream::connect(socket).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            stream.write_all(b"zoom-out\n").unwrap();
            let mut reply = [0; 3];
            stream.read_exact(&mut reply).unwrap();
            assert_eq!(&reply, b"ok\n");
            clicked.set(true);
        }
    ));
    window.set_child(Some(&button));
    glib::timeout_add_local(
        std::time::Duration::from_millis(25),
        glib::clone!(
            #[weak]
            window,
            #[weak]
            button,
            #[strong]
            clicked,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                let bounds = button.compute_bounds(&window).map(|bounds| serde_json::json!({
                    "x": bounds.x(), "y": bounds.y(), "width": bounds.width(), "height": bounds.height(),
                }));
                let state = serde_json::json!({"active": window.is_active(), "mapped": window.is_mapped(), "clicked": clicked.get(), "button": bounds});
                let temporary = path.with_extension("json.tmp");
                std::fs::write(&temporary, serde_json::to_vec(&state).unwrap()).unwrap();
                std::fs::rename(temporary, &path).unwrap();
                glib::ControlFlow::Continue
            }
        ),
    );
    window.present();
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
    // Register the popup with TextView's child lifecycle. Setting only its
    // parent leaves GTK 4.14 unable to remove it when the window closes.
    editor.add_overlay(&popover, 0, 0);
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
    if std::env::var_os("HEARTHSPACE_WINDOW_TEST_MAXIMIZED").is_some() {
        window.maximize();
    }
    window.present();
    editor.grab_focus();
}
