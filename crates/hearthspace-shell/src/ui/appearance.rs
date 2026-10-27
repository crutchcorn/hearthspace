//! One immutable property set per live shell appearance; failed reads keep it.
use super::state::ShellState;
use hearthspace_ui_components::{Palette, PropertyStyle};
use std::sync::Arc;
use xilem::{
    core::{MessageProxy, NoElement, View},
    masonry::core::DefaultProperties,
    view::task_raw,
};

mod classes;
pub(super) use classes::{ThemeStyle, Tone};

pub(super) struct Theme {
    pub palette: Palette,
    pub properties: Arc<DefaultProperties>,
}
impl Default for Theme {
    fn default() -> Self {
        Self::new(Palette::default())
    }
}
impl Theme {
    fn new(palette: Palette) -> Self {
        Self {
            palette,
            properties: Arc::new(properties(palette)),
        }
    }
    pub(super) fn update(&mut self, palette: Palette) {
        if self.palette != palette {
            *self = Self::new(palette);
        }
    }
}

pub(super) fn properties(palette: Palette) -> DefaultProperties {
    let palette = palette.for_glass();
    use xilem::masonry::{
        properties::{
            BorderColor, CaretColor, ContentColor, SelectionColor, ThumbColor, TrackColor,
        },
        widgets::{Label, SizedBox},
    };
    let mut properties =
        hearthspace_ui_components::hearthspace_properties(palette, PropertyStyle::Glass);
    properties.insert_stack::<Label>(classes::properties(&palette));
    properties.insert_stack::<SizedBox>(classes::properties(&palette));
    properties.insert_stack::<hearthspace_ui_components::icons::IconWidget>(classes::properties(
        &palette,
    ));
    super::search_result::theme_properties(&mut properties, palette, classes::properties(&palette));
    super::minimap::theme_properties(&mut properties, palette);
    super::popup::theme_properties(&mut properties, palette);
    super::dock::theme_properties(&mut properties, palette);
    super::desktop_item::theme_properties(&mut properties, palette);
    properties.insert::<super::tray_widget::TrayButton, _>(palette);
    properties.insert::<super::search_editor::SearchEditor, _>(ContentColor::new(palette.text));
    properties.insert::<super::search_editor::SearchEditor, _>(CaretColor {
        color: palette.text,
    });
    properties.insert::<super::search_editor::SearchEditor, _>(SelectionColor {
        color: palette.accent_soft,
    });
    properties.insert_stack::<super::search_editor::SearchEditor>(classes::properties(&palette));
    properties.insert::<super::control_slider::NamedSlider, _>(TrackColor {
        active: palette.accent,
        inactive: palette.border,
    });
    properties.insert::<super::control_slider::NamedSlider, _>(ThumbColor(palette.surface));
    properties.insert::<super::control_slider::NamedSlider, _>(BorderColor {
        color: palette.focus_ring_on_glass,
    });
    properties
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Palette>, _: &mut ShellState| async move {
            use hearthspace_ipc::settings_snapshot::{client_get_snapshot, client_wait_for_change};
            let mut previous = None;
            let mut cursor = None;
            loop {
                let after = cursor.clone();
                let result = tokio::task::spawn_blocking(move || match after {
                    Some(after) => {
                        client_wait_for_change(&after, std::time::Duration::from_millis(500))
                    }
                    None => client_get_snapshot().map(Some),
                })
                .await;
                match result {
                    Ok(Ok(Some(snapshot))) => {
                        cursor = Some(snapshot.cursor());
                        let settings = snapshot.settings;
                        let palette = Palette::new(settings.appearance)
                            .with_photo(settings.background_image_path.is_some());
                        if previous != Some(palette) {
                            previous = Some(palette);
                            if proxy.message(palette).is_err() {
                                return;
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    _ => {
                        cursor = None;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        },
        |state, palette| state.appearance.update(palette),
    )
}

#[cfg(test)]
mod tests;
