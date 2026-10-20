//! A lightweight live map of the infinite canvas, kept separate from the
//! compositor's window previews. All drawing remains in the shell client.

use crate::ui::appearance::{ThemeStyle as _, Tone};
use xilem::{WidgetView, masonry::layout::AsUnit, style::Style, view::sized_box};

use super::state::ShellState;

mod model;
mod projection;
mod render;
mod widget;

pub(super) use model::{CanvasSnapshot, parse_snapshot};
use projection::Projection;

#[cfg(test)]
mod tests;

pub(super) fn minimap_view(
    snapshot: Option<&CanvasSnapshot>,
    output_name: Option<&str>,
) -> impl WidgetView<ShellState> + use<> {
    sized_box(widget::MinimapView {
        snapshot: snapshot.cloned(),
        output_name: output_name.map(str::to_owned),
    })
    .fixed_height(190.0.px())
    .theme_background(Tone::Subtle)
    .theme_border(Tone::Border)
    .border_width(1.0.px())
    .corner_radius(6.0.px())
}

fn description(snapshot: Option<&CanvasSnapshot>, output_name: Option<&str>) -> String {
    let Some(snapshot) = snapshot else {
        return "Canvas minimap loading".into();
    };
    let mut description = format!(
        "Canvas minimap: {} windows. Accent outline shows the visible area{}.",
        snapshot.windows.len(),
        output_name
            .map(|name| format!(" on {name}"))
            .unwrap_or_default(),
    );
    if let Some(window) = snapshot.windows.iter().find(|window| window.focused) {
        let name = if window.title.is_empty() {
            &window.app_id
        } else {
            &window.title
        };
        if !name.is_empty() {
            description.push_str(&format!(" Focused window: {name}."));
        }
    }
    description
}

pub(super) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    properties.insert::<widget::Minimap, _>(palette);
}
