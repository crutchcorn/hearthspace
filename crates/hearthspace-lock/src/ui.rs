//! Presentation only. The session owns authentication and the unlock capability.
use std::{cell::Cell, rc::Rc, time::Duration};

use gtk::{gdk, glib, prelude::*};
use hearthspace_ui_components::{Palette, icons::Icon};
use zeroize::Zeroizing;

use crate::config::Config;
#[path = "ui/elements.rs"]
mod elements;
#[path = "ui/secret.rs"]
mod secret;
#[path = "ui/status.rs"]
mod status;

pub(crate) struct Screen {
    pub window: gtk::Window,
    entry: gtk::PasswordEntry,
    submit: gtk::Button,
    feedback: gtk::Label,
    row: gtk::Box,
    busy: Rc<Cell<bool>>,
    caps: Rc<Cell<bool>>,
    _style: crate::theme::Style,
}

impl Screen {
    pub fn new(
        config: &Config,
        monitor: &gdk::Monitor,
        on_submit: impl Fn(Zeroizing<String>) + 'static,
    ) -> Self {
        let palette = Palette::new(config.settings.appearance)
            .with_photo(config.settings.background_image_path.is_some());
        let style = crate::theme::install(&monitor.display(), palette);
        let window = gtk::Window::builder()
            .title("Hearthspace Lock Screen")
            .decorated(false)
            .css_classes(["lock-screen"])
            .build();
        // A close request is never an authentication result.
        window.connect_close_request(|_| glib::Propagation::Stop);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&crate::background::widget(config, monitor)));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add_css_class("content");
        content.append(&status::header(palette));

        let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center.set_valign(gtk::Align::Center);
        center.set_halign(gtk::Align::Center);
        center.set_vexpand(true);
        let (clock, time) = elements::clock();
        center.append(&clock);
        let form = gtk::Box::new(gtk::Orientation::Vertical, 0);
        form.set_halign(gtk::Align::Center);
        let avatar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        avatar.add_css_class("avatar");
        avatar.set_halign(gtk::Align::Center);
        avatar.set_margin_bottom(16);
        let user = elements::icon(Icon::User, 30, palette.accent_decorative);
        user.set_halign(gtk::Align::Center);
        user.set_valign(gtk::Align::Center);
        user.set_vexpand(true);
        avatar.append(&user);
        form.append(&avatar);
        let name = elements::label(&config.display_name, "name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(26);
        form.append(&name);
        let welcome = elements::label("Welcome back.", "welcome");
        welcome.set_margin_top(8);
        welcome.set_margin_bottom(24);
        form.append(&welcome);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.add_css_class("password-row");
        let entry = gtk::PasswordEntry::builder()
            .placeholder_text("Password")
            .show_peek_icon(false)
            .hexpand(true)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label("Password")]);
        let submit = gtk::Button::builder()
            .css_classes(["submit"])
            .tooltip_text("Unlock")
            .focus_on_click(false)
            .sensitive(false)
            .child(&elements::icon(Icon::ChevronRight, 20, palette.accent_ink))
            .build();
        submit.update_property(&[gtk::accessible::Property::Label("Unlock")]);
        row.append(&entry);
        row.append(&elements::password_toggle(&entry, palette));
        row.append(&submit);
        form.append(&row);
        let feedback = elements::label("\u{a0}", "feedback");
        feedback.set_margin_top(12);
        feedback.set_wrap(true);
        feedback.set_justify(gtk::Justification::Center);
        feedback.set_max_width_chars(42);
        form.append(&feedback);
        center.append(&form);
        content.append(&center);
        overlay.add_overlay(&content);
        window.set_child(Some(&overlay));
        gtk::prelude::GtkWindowExt::set_focus(&window, Some(&entry));

        let busy = Rc::new(Cell::new(false));
        let callback: Rc<dyn Fn(Zeroizing<String>)> = Rc::new(on_submit);
        let activate = {
            let entry = entry.downgrade();
            let busy = busy.clone();
            move || {
                let Some(entry) = entry.upgrade() else {
                    return;
                };
                if busy.get() || secret::is_empty(&entry) {
                    return;
                }
                if let Some(password) = secret::take(&entry) {
                    callback(password);
                }
            }
        };
        let activate = Rc::new(activate);
        let clicked = activate.clone();
        submit.connect_clicked(move |_| clicked());
        entry.connect_activate(move |_| activate());
        let enabled = submit.downgrade();
        let changed_row = row.downgrade();
        let changed_feedback = feedback.downgrade();
        let changed_busy = busy.clone();
        let caps = Rc::new(Cell::new(false));
        let changed_caps = caps.clone();
        let active_arrow = elements::icon(Icon::ChevronRight, 20, palette.accent_ink);
        let inactive_arrow = elements::icon(Icon::ChevronRight, 20, palette.muted);
        submit.set_child(Some(&inactive_arrow));
        entry.connect_changed(move |entry| {
            let empty = secret::is_empty(entry);
            if let Some(button) = enabled.upgrade() {
                let enabled = !changed_busy.get() && !empty;
                button.set_sensitive(enabled);
                button.set_child(Some(if enabled {
                    &active_arrow
                } else {
                    &inactive_arrow
                }));
            }
            if !empty {
                if let Some(row) = changed_row.upgrade() {
                    row.remove_css_class("error");
                }
                if let Some(label) = changed_feedback.upgrade() {
                    label.set_label(elements::caps_feedback(changed_caps.get()));
                }
                entry.update_state(&[gtk::accessible::State::Invalid(
                    gtk::AccessibleInvalidState::False,
                )]);
            }
        });
        elements::caps_lock(&window, &feedback, &row, caps.clone(), busy.clone());
        let size = monitor.geometry();
        elements::resize(
            &window,
            &center,
            &form,
            &avatar,
            &time,
            size.width(),
            size.height(),
        );
        let previous = Cell::new((size.width(), size.height()));
        window.add_tick_callback(move |window, _| {
            let size = (window.width(), window.height());
            if size.0 > 0 && size.1 > 0 && size != previous.replace(size) {
                elements::resize(window, &center, &form, &avatar, &time, size.0, size.1);
            }
            glib::ControlFlow::Continue
        });
        // A bounded opacity fade reveals only our opaque wallpaper; the
        // compositor supplies black underneath, never desktop application pixels.
        if gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()) {
            elements::fade_in(&window, Duration::from_millis(220));
        }
        Self {
            window,
            entry,
            submit,
            feedback,
            row,
            busy,
            caps,
            _style: style,
        }
    }

    pub fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        self.entry.set_sensitive(!busy);
        self.submit
            .set_sensitive(!busy && !secret::is_empty(&self.entry));
        if busy {
            self.feedback.set_label("Checking…");
        } else {
            self.entry.grab_focus();
        }
    }

    pub fn set_feedback(&self, text: &str) {
        self.feedback.set_label(if text.is_empty() {
            elements::caps_feedback(self.caps.get())
        } else {
            text
        });
        if text.is_empty() {
            self.row.remove_css_class("error");
        } else {
            self.row.add_css_class("error");
        }
        self.entry
            .update_state(&[gtk::accessible::State::Invalid(if text.is_empty() {
                gtk::AccessibleInvalidState::False
            } else {
                gtk::AccessibleInvalidState::True
            })]);
    }

    pub fn clear_secret(&self) {
        secret::clear(&self.entry);
    }
}
