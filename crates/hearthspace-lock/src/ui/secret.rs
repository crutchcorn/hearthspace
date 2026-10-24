//! Keep secret bytes out of GTK-rs' ordinary owned GString getters.

use std::ffi::CStr;

use gtk::{glib::translate::ToGlibPtr, prelude::*};
use zeroize::Zeroizing;

fn text(entry: &gtk::PasswordEntry) -> gtk::Text {
    // GtkPasswordEntry delegates GtkEditable to GtkText. Use the public
    // delegation API, not child positions or private GTK struct fields. If a
    // future GTK changes this contract, stop rather than read or reveal secrets.
    entry
        .delegate()
        .and_then(|delegate| delegate.downcast::<gtk::Text>().ok())
        .expect("GtkPasswordEntry must have a GtkText delegate")
}

pub(super) fn is_empty(entry: &gtk::PasswordEntry) -> bool {
    text(entry).text_length() == 0
}

pub(super) fn clear(entry: &gtk::PasswordEntry) {
    // Reveal is a persistent GtkText property, independent of buffer contents.
    // Its visibility notification also updates PasswordEntry's peek icon. Mask
    // before deleting text so even synchronous changed handlers see a masked field.
    text(entry).set_visibility(false);
    entry.set_text("");
}

pub(super) fn take(entry: &gtk::PasswordEntry) -> Option<Zeroizing<String>> {
    let mut secret = Zeroizing::new(String::new());
    let copied = {
        // SAFETY: the live PasswordEntry implements GtkEditable. GTK owns the
        // returned NUL-terminated UTF-8 string; GTK widgets can only be used on
        // the main thread. We neither mutate the widget nor run GTK/a main-loop
        // iteration while borrowing its bytes. No borrowed pointer/string escapes
        // this block. We copy directly into an already-zeroizing allocation,
        // reserving its final size before it contains any secret. In particular,
        // EditableExt::text()/GString/to_string_lossy() must not be used here.
        unsafe {
            let pointer = gtk::ffi::gtk_editable_get_text(
                entry.upcast_ref::<gtk::Editable>().to_glib_none().0,
            );
            if pointer.is_null() {
                false
            } else if let Ok(borrowed) = CStr::from_ptr(pointer).to_str() {
                secret.reserve_exact(borrowed.len());
                secret.push_str(borrowed);
                true
            } else {
                false
            }
        }
    };
    // End the GTK-owned borrow before any mutation or synchronous signal.
    clear(entry);
    if copied && !secret.is_empty() {
        Some(secret)
    } else {
        None
    }
}
