//! Cache native defaults by appearance without replacing the window's widgets.
use hearthspace_ipc::Appearance;
use hearthspace_ui_components::{self as components, Palette, PropertyStyle};
use std::{
    cell::RefCell,
    sync::{Arc, atomic::AtomicU64},
};
use xilem::masonry::{
    core::{DefaultProperties, PropertyStack, Selector},
    properties::{CaretColor, ContentColor, SelectionColor},
    widgets::Label,
};

#[derive(Default)]
pub(crate) struct Theme {
    cached: RefCell<Option<Resolved>>,
    pub(super) revision: Arc<AtomicU64>,
}

struct Resolved {
    palette: Palette,
    properties: Arc<DefaultProperties>,
}

impl Theme {
    fn resolve(&self, appearance: Appearance, photo: bool) {
        let mut cache = self.cached.borrow_mut();
        if cache.as_ref().is_some_and(|value| {
            value.palette.appearance == appearance && value.palette.photo == photo
        }) {
            return;
        }
        let palette = Palette::new(appearance).with_photo(photo);
        let properties = properties(palette);
        *cache = Some(Resolved {
            palette,
            properties: Arc::new(properties),
        });
    }

    pub(crate) fn palette(&self, appearance: Appearance, photo: bool) -> Palette {
        self.resolve(appearance, photo);
        self.cached.borrow().as_ref().unwrap().palette
    }

    pub(crate) fn properties(&self, appearance: Appearance, photo: bool) -> Arc<DefaultProperties> {
        self.resolve(appearance, photo);
        self.cached.borrow().as_ref().unwrap().properties.clone()
    }
}

pub(crate) fn properties(palette: Palette) -> DefaultProperties {
    let mut defaults = components::hearthspace_properties(palette, PropertyStyle::Opaque);
    let properties = &mut defaults;
    properties.insert::<crate::window_frame::WindowBody, _>(palette);
    properties.insert::<crate::monitor_layout_widget::MonitorLayoutWidget, _>(palette);
    crate::page_components::theme_properties(properties, palette);
    crate::favorites_view::theme_properties(properties, palette);
    crate::ai_view::theme_properties(properties, palette);
    hearthspace_shell::gesture_pad::theme_properties(properties, palette);
    properties.insert::<crate::favorites_search::Editor, _>(ContentColor::new(palette.text));
    properties.insert::<crate::favorites_search::Editor, _>(CaretColor {
        color: palette.text,
    });
    properties.insert::<crate::favorites_search::Editor, _>(SelectionColor {
        color: palette.accent_soft,
    });
    fn colors(palette: Palette) -> PropertyStack {
        let mut stack = PropertyStack::new();
        for (class, color) in [
            ("hearthspace-muted-text", palette.text_muted),
            ("settings-muted", palette.muted),
            ("settings-accent", palette.accent),
            ("settings-ink", palette.accent_ink),
        ] {
            stack.push_layer(Selector::classes(&[class]), ContentColor::new(color));
        }
        stack
    }
    properties.insert_stack::<Label>(colors(palette));
    properties.insert_stack::<components::icons::IconWidget>(colors(palette));
    defaults
}
