use gtk::{gdk, glib, prelude::*};
use hearthspace_ui_components::{
    Color,
    icons::{Icon, IconGlyph},
};
use std::time::{Duration, Instant};

pub(super) fn icon(kind: Icon, size: i32, color: Color) -> gtk::Image {
    let svg = IconGlyph::new(kind, false).svg(f64::from(size), color);
    let bytes = glib::Bytes::from_owned(svg.into_bytes());
    let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, None::<&gtk::gio::Cancellable>)
        .expect("bundled Hearthspace SVG");
    let image = gtk::Image::from_paintable(Some(&gdk::Texture::for_pixbuf(&pixbuf)));
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    image
}

pub(super) fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(class);
    label
}

pub(super) fn password_toggle(
    entry: &gtk::PasswordEntry,
    palette: hearthspace_ui_components::Palette,
) -> gtk::Button {
    let button = gtk::Button::builder()
        .css_classes(["reveal"])
        .tooltip_text("Show password")
        .focus_on_click(false)
        .child(&icon(Icon::Eye, 18, palette.muted))
        .build();
    button.update_property(&[gtk::accessible::Property::Label("Show password")]);
    let text = entry.delegate().unwrap().downcast::<gtk::Text>().unwrap();
    let target = text.downgrade();
    button.connect_clicked(move |_| {
        if let Some(text) = target.upgrade() {
            text.set_visibility(!text.is_visible());
        }
    });
    let target = button.downgrade();
    text.connect_visibility_notify(move |text| {
        if let Some(button) = target.upgrade() {
            let label = if text.is_visible() {
                "Hide password"
            } else {
                "Show password"
            };
            button.set_tooltip_text(Some(label));
            button.update_property(&[gtk::accessible::Property::Label(label)]);
        }
    });
    button
}

pub(super) fn clock() -> (gtk::Box, gtk::Label) {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let time = label("", "time");
    let date = label("", "date");
    group.append(&time);
    group.append(&date);
    let updating_time = time.clone();
    let update = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            updating_time.set_label(now.format("%H:%M").as_deref().unwrap_or(""));
            date.set_label(now.format("%A, %B %-d").as_deref().unwrap_or(""));
        }
    };
    update();
    let weak = group.downgrade();
    glib::timeout_add_local(Duration::from_secs(15), move || {
        if weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        update();
        glib::ControlFlow::Continue
    });
    (group, time)
}

pub(super) fn resize(
    window: &gtk::Window,
    center: &gtk::Box,
    form: &gtk::Box,
    avatar: &gtk::Box,
    time: &gtk::Label,
    width: i32,
    height: i32,
) {
    let narrow = width <= 480;
    let compact = height <= 600;
    let time_height = if compact {
        64
    } else if narrow {
        68
    } else {
        112
    };
    let attrs = gtk::pango::AttrList::new();
    attrs.insert(gtk::pango::AttrInt::new_line_height_absolute(
        time_height * gtk::pango::SCALE,
    ));
    time.set_attributes(Some(&attrs));
    for (class, active) in [("narrow", narrow), ("compact", compact)] {
        if active {
            window.add_css_class(class);
        } else {
            window.remove_css_class(class);
        }
    }
    avatar.set_visible(!compact);
    form.set_size_request((width - if narrow { 48 } else { 64 }).clamp(160, 340), -1);
    center.set_spacing(if compact {
        24
    } else {
        (height * 8 / 100).clamp(36, 84)
    });
    center.set_margin_top(if compact { 24 } else { 40 });
    center.set_margin_bottom(if compact { 24 } else { 40 });
}

pub(super) fn caps_feedback(active: bool) -> &'static str {
    if active { "Caps Lock is on." } else { "\u{a0}" }
}

pub(super) fn caps_lock(
    window: &gtk::Window,
    feedback: &gtk::Label,
    row: &gtk::Box,
    caps: std::rc::Rc<std::cell::Cell<bool>>,
    busy: std::rc::Rc<std::cell::Cell<bool>>,
) {
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let feedback = feedback.downgrade();
    let row = row.downgrade();
    key.connect_modifiers(move |_, modifiers| {
        caps.set(modifiers.contains(gdk::ModifierType::LOCK_MASK));
        if let (Some(label), Some(row)) = (feedback.upgrade(), row.upgrade())
            && !busy.get()
            && !row.has_css_class("error")
        {
            label.set_label(caps_feedback(caps.get()));
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key);
}

pub(super) fn fade_in(window: &gtk::Window, duration: Duration) {
    let start = CellStart::default();
    window.set_opacity(0.0);
    window.add_tick_callback(move |window, _| {
        let elapsed =
            start.0.get_or_init(Instant::now).elapsed().as_secs_f64() / duration.as_secs_f64();
        window.set_opacity(1.0 - (1.0 - elapsed.min(1.0)).powi(3));
        if elapsed >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

#[derive(Default)]
struct CellStart(std::cell::OnceCell<Instant>);
