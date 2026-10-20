//! Conversions at the compositor's renderer boundary.
use hearthspace_ui_components::Palette;
use smithay::backend::renderer::Color32F;

impl super::App {
    pub(super) fn palette(&self) -> Palette {
        self.palette
            .with_photo(self.background_image_path.is_some())
    }
}

pub(super) fn color(value: xilem::Color) -> Color32F {
    let [r, g, b, a] = value.components;
    Color32F::new(r * a, g * a, b * a, a)
}
