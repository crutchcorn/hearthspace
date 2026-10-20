//! Keep font discovery and font files out of hover/press raster rebuilds.
use std::cell::RefCell;

use masonry::parley::FontContext;

thread_local! {
    // Parley recommends one context per application or thread. Collection clones
    // share Fontique's system database; SourceCache clones retain loaded blobs.
    static FONTS: RefCell<Option<FontContext>> = const { RefCell::new(None) };
}

pub(super) fn install(context: &mut FontContext) {
    FONTS.with(|cached| {
        let mut cached = cached.borrow_mut();
        let fonts = cached.get_or_insert_with(|| {
            let mut fonts = context.clone();
            fonts.collection.load_system_fonts();
            fonts
        });
        *context = fonts.clone();
    });
}

pub(super) fn preserve(context: &FontContext) {
    FONTS.with(|cached| *cached.borrow_mut() = Some(context.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::{
        masonry_titlebar::{TitlebarBuffer, TitlebarState},
        windows::TitlebarButton,
    };

    #[test]
    fn hover_redraw_reuses_discovered_family_and_loaded_font_bytes() {
        let mut state = TitlebarState {
            appearance: hearthspace_ipc::Appearance::default(),
            title: "Hearthspace Settings".into(),
            app_id: Some("dev.hearthspace.Settings".into()),
            active: true,
            maximized: false,
            hovered: None,
            pressed: None,
            photo: false,
        };
        let mut cache = TitlebarBuffer::new(state.clone(), 1);
        let initial = font_identity();
        state.hovered = Some(TitlebarButton::Close);
        cache.update(state, 1);
        assert_eq!(
            font_identity(),
            initial,
            "hover redraw must retain both the discovered family and its loaded font blob"
        );
    }

    fn font_identity() -> (u64, u64) {
        FONTS.with(|cached| {
            let mut cached = cached.borrow_mut();
            let fonts = cached.as_mut().expect("render populated the font cache");
            let family = fonts
                .collection
                .family_by_name("Liberation Sans")
                .expect("the documented Settings font is installed");
            let font = family.default_font().unwrap();
            let bytes = fonts.source_cache.get(font.source()).unwrap();
            (family.id().to_u64(), bytes.id())
        })
    }
}
