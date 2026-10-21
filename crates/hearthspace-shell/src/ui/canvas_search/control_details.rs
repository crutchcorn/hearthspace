//! Compact native artwork strips mirror the reference's secondary detail row.
use crate::{
    app_catalog::DesktopApp,
    ui::{desktop, dock, state::ShellState},
};
use hearthspace_ui_components::icons::{Icon, icon};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, flex_row, image, label, sized_box},
};

pub(super) fn view(
    state: &ShellState,
    id: &str,
    text: Option<String>,
) -> Option<Box<AnyWidgetView<ShellState>>> {
    match id {
        "control:apps" => Some(apps(state).boxed()),
        "control:canvas" => Some(tools(state).boxed()),
        _ => text.map(|text| {
            label(text)
                .text_size(12.0)
                .color(state.appearance.palette.muted)
                .line_break_mode(LineBreaking::Clip)
                .height(18.0.px())
                .boxed()
        }),
    }
}

fn preview_apps(state: &ShellState) -> Vec<&DesktopApp> {
    let favorites: Vec<_> = state
        .favorites
        .store
        .entries()
        .iter()
        .filter_map(|favorite| state.catalog.app_by_id(&favorite.app_id))
        .take(4)
        .collect();
    if favorites.is_empty() {
        state.catalog.apps().iter().take(4).collect()
    } else {
        favorites
    }
}

fn apps(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let artwork: Vec<_> = preview_apps(state)
        .into_iter()
        .map(|app| {
            let artwork: Box<AnyWidgetView<ShellState>> =
                if app.id == "dev.hearthspace.Settings.desktop" {
                    icon(Icon::AppSettings, 18.0)
                        .color(state.appearance.palette.text)
                        .boxed()
                } else if let Some(brush) = dock::image(state, app.icon.as_deref()) {
                    image(brush).decorative(true).boxed()
                } else {
                    icon(Icon::AppsGrid, 18.0)
                        .color(state.appearance.palette.muted)
                        .boxed()
                };
            sized_box(artwork).width(18.0.px()).height(18.0.px())
        })
        .collect();
    flex_row(artwork)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(8.0.px())
        .height(18.0.px())
}

fn tools(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let palette = state.appearance.palette;
    let tools = [
        (Icon::Cursor, desktop::selection_active(state)),
        (Icon::Search, state.canvas_search.snapshot.mode),
        (Icon::Lasso, state.window_group.selecting),
        (
            Icon::Overview,
            state.outputs.iter().any(|output| output.show_map),
        ),
    ]
    .into_iter()
    .map(|(glyph, active)| {
        icon(glyph, 16.0).color(if active { palette.text } else { palette.muted })
    })
    .collect::<Vec<_>>();
    flex_row(tools)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(8.0.px())
        .height(18.0.px())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_previews_follow_installed_favorites_then_fall_back_to_real_catalog() {
        let directory = tempfile::tempdir().unwrap();
        let applications = directory.path().join("applications");
        std::fs::create_dir(&applications).unwrap();
        for name in ["Alpha", "Bravo", "Charlie", "Delta", "Echo"] {
            std::fs::write(
                applications.join(format!("{name}.desktop")),
                format!("[Desktop Entry]\nType=Application\nName={name}\nExec=true\nIcon={name}\n"),
            )
            .unwrap();
        }
        let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
        state.catalog =
            crate::app_catalog::AppCatalog::load_from_data_dirs(vec![directory.path().into()]);
        let names = |state: &ShellState| {
            preview_apps(state)
                .iter()
                .map(|app| app.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&state), ["Alpha", "Bravo", "Charlie", "Delta"]);
        state.favorites.store.add("missing.desktop").unwrap();
        assert_eq!(names(&state), ["Alpha", "Bravo", "Charlie", "Delta"]);
        state.favorites.store.add("Echo.desktop").unwrap();
        state.favorites.store.add("Bravo.desktop").unwrap();
        assert_eq!(names(&state), ["Echo", "Bravo"]);

        let ui = crate::ui::test_support::Ui::new(
            state,
            |state| {
                sized_box(super::super::view(
                    state,
                    super::super::ENTRIES
                        .iter()
                        .find(|entry| entry.id == "control:apps")
                        .unwrap(),
                ))
            },
            (240, 72),
        );
        assert!(ui.harness.access_node(ui.button("Apps")).is_some());
        let mut icons: Vec<_> = crate::ui::test_support::ids(&ui.harness)
            .into_iter()
            .filter_map(|id| ui.harness.access_node(id))
            .filter(|node| node.role() == masonry::accesskit::Role::Image)
            .filter_map(|node| node.bounding_box())
            .filter(|bounds| bounds.width() == 18.0)
            .collect();
        icons.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        assert_eq!(icons.len(), 2);
        assert_eq!(icons[0].height(), 18.0);
        assert_eq!(icons[1].x0 - icons[0].x1, 8.0);
        assert!(
            icons
                .iter()
                .all(|bounds| bounds.x1 <= 240.0 && bounds.y1 <= 72.0)
        );
    }
}
