//! Wallpaper fitting uses the same floating select as display scale.
use super::select::select;
use crate::{actions::save_background_mode, state::SettingsApp};
use hearthspace_ipc::BackgroundMode;
use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::sized_box,
};

const MODES: [(BackgroundMode, &str); 4] = [
    (BackgroundMode::RepeatWithCanvas, "Repeat with canvas"),
    (BackgroundMode::Stretch, "Stretch"),
    (BackgroundMode::Contains, "Contain"),
    (BackgroundMode::Cover, "Cover"),
];

pub(crate) fn photo_fit(selected: BackgroundMode, disabled: bool) -> impl WidgetView<SettingsApp> {
    let selected = MODES
        .iter()
        .position(|(mode, _)| *mode == selected)
        .unwrap();
    sized_box(
        select(
            "Photo fit",
            MODES.map(|(_, label)| label.to_owned()).to_vec(),
            selected,
            disabled,
            |state, index| save_background_mode(state, MODES[index].0),
        )
        .padding(Padding {
            top: 6.0.px(),
            bottom: 6.0.px(),
            left: 12.0.px(),
            right: 2.0.px(),
        }),
    )
    // Like an HTML select, keep room for the longest choice after selection.
    .fixed_width(134.0.px())
    .fixed_height(34.0.px())
}
