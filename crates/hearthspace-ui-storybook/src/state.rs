use hearthspace_common::{AccentColor, Appearance, ColorMode};
use hearthspace_ui_components::{Palette, PropertyStyle, hearthspace_properties};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub(super) struct AppState {
    pub running: bool,
    pub catalog: xilem_storybook::StorybookState,
    pub dark: bool,
    pub accent: usize,
    pub clicks: usize,
    pub properties: Arc<xilem::masonry::core::DefaultProperties>,
    previous: Appearance,
}
impl AppState {
    pub fn new() -> Self {
        let appearance = Appearance::default();
        let mut stories = super::shared::stories();
        stories.extend(super::catalog::initial_state().stories().iter().cloned());
        Self {
            running: true,
            catalog: xilem_storybook::StorybookState::new(stories),
            dark: false,
            accent: 0,
            clicks: 0,
            properties: Arc::new(hearthspace_properties(
                Palette::new(appearance),
                PropertyStyle::Opaque,
            )),
            previous: appearance,
        }
    }
    pub fn palette(&self) -> Palette {
        Palette::new(self.appearance())
    }
    fn appearance(&self) -> Appearance {
        Appearance {
            color_mode: if self.dark {
                ColorMode::Dark
            } else {
                ColorMode::Light
            },
            accent_color: AccentColor::ALL[self.accent],
        }
    }
    pub fn update_theme(&mut self) {
        let a = self.appearance();
        if a != self.previous {
            self.properties = Arc::new(hearthspace_properties(
                Palette::new(a),
                PropertyStyle::Opaque,
            ));
            self.previous = a;
        }
    }
}
impl Deref for AppState {
    type Target = xilem_storybook::StorybookState;
    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}
impl DerefMut for AppState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.catalog
    }
}

impl xilem::AppState for AppState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_catalog_covers_all_named_states_and_retains_edits_through_theme_changes() {
        let mut state = AppState::new();
        assert_eq!(
            state
                .stories()
                .iter()
                .filter(|s| s.id().starts_with("shared-controls-"))
                .count(),
            23
        );
        let i = state
            .stories()
            .iter()
            .position(|s| s.id() == "shared-controls-text-input--filled")
            .unwrap();
        state.select_story(i);
        state.set_selected_text("value", "Edited network".into());
        let old = state.properties.clone();
        state.dark = true;
        state.accent = 3;
        state.update_theme();
        assert!(state.palette().dark);
        assert!(!Arc::ptr_eq(&old, &state.properties));
        assert_eq!(state.selected_text("value"), Some("Edited network"));
        let themed = state.properties.clone();
        state.update_theme();
        assert!(Arc::ptr_eq(&themed, &state.properties));
        state.select_story(0);
        state.select_story(i);
        assert_eq!(state.selected_text("value"), Some("Edited network"));
    }
    #[test]
    fn masked_samples_do_not_offer_an_editable_plaintext_value() {
        let state = AppState::new();
        for id in [
            "shared-controls-text-input--password",
            "shared-controls-text-input--invalid",
        ] {
            let story = state.stories().iter().find(|s| s.id() == id).unwrap();
            assert!(story.controls().iter().all(|c| c.id() != "value"));
            assert!(story.description().contains("visual sample"));
        }
    }
}
