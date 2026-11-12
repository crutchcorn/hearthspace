//! Use the same keyboard-device state and realization lifetime as GTK's icon.
use gtk::{gdk, prelude::*};
use std::{cell::Cell, rc::Rc};

pub(super) fn bind(
    entry: &gtk::PasswordEntry,
    feedback: &gtk::Label,
    row: &gtk::Box,
    caps: Rc<Cell<bool>>,
    busy: Rc<Cell<bool>>,
) {
    let feedback = feedback.downgrade();
    let row = row.downgrade();
    let refresh = Rc::new(move |device: &glib::Object| {
        let keyboard = device.downcast_ref::<gdk::Device>().unwrap();
        caps.set(keyboard.is_caps_locked());
        if let (Some(label), Some(row)) = (feedback.upgrade(), row.upgrade())
            && !busy.get()
            && !row.has_css_class("error")
        {
            label.set_label(super::elements::caps_feedback(caps.get()));
        }
    });
    let signals = glib::SignalGroup::new::<gdk::Device>();
    let update = refresh.clone();
    signals.connect_notify_local(Some("caps-lock-state"), move |device, _| update(device));
    signals.connect_bind_local(move |_, device| refresh(device));
    let realized = signals.clone();
    entry.connect_realize(move |entry| {
        let keyboard = entry
            .display()
            .default_seat()
            .and_then(|seat| seat.keyboard());
        realized.set_target(keyboard.as_ref());
    });
    entry.connect_unrealize(move |_| signals.set_target(glib::Object::NONE));
}
