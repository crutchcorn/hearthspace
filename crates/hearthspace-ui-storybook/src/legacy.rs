use super::*;
pub(super) fn preview(state: &StorybookState) -> Box<AnyWidgetView<StorybookState>> {
    let story = state.selected_story();
    let content = match story.id() {
        "settings-materials" => settings_materials::preview(state).boxed(),
        "page-title" => components::page_title(text(state, "text")).boxed(),
        "section-title" => components::section_title(text(state, "text")).boxed(),
        "muted-text" => components::muted_text(text(state, "text")).boxed(),
        "action-button" => {
            components::action_button(text(state, "text"), |state: &mut StorybookState| {
                state.set_selected_text("text", "Clicked action button".to_string());
            })
            .boxed()
        }
        "compact-action-button" => {
            components::compact_action_button(text(state, "text"), |state: &mut StorybookState| {
                state.set_selected_text("text", "Clicked compact button".to_string());
            })
            .boxed()
        }
        "segmented-option" => components::segmented_option(
            text(state, "text"),
            bool_value(state, "selected"),
            |state: &mut StorybookState| {
                let selected = state.selected_bool("selected").unwrap_or(false);
                state.set_selected_bool("selected", !selected);
            },
        )
        .boxed(),
        "sidebar-item" => components::sidebar_item(
            text(state, "text"),
            bool_value(state, "selected"),
            |state: &mut StorybookState| {
                let selected = state.selected_bool("selected").unwrap_or(false);
                state.set_selected_bool("selected", !selected);
            },
        )
        .boxed(),
        "settings-section" => components::settings_section(
            text(state, "title"),
            flex_col((
                components::muted_text(text(state, "body")),
                components::action_button("Example action", |_| {}),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(10.0.px()),
        )
        .boxed(),
        "sidebar-panel" => components::sidebar_panel(
            flex_col((
                components::page_title("Settings"),
                components::sidebar_item(text(state, "label"), true, |_| {}),
                components::sidebar_item("Appearance", false, |_| {}),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        )
        .boxed(),
        "launcher-item" => {
            components::launcher_item(text(state, "text"), 320.0, |state: &mut StorybookState| {
                state.set_selected_text("text", "Clicked launcher item".to_string());
            })
            .boxed()
        }
        _ => components::muted_text("Unknown story").boxed(),
    };

    sized_box(
        flex_col((
            FlexSpacer::Flex(1.0),
            flex_row((FlexSpacer::Flex(1.0), content, FlexSpacer::Flex(1.0))),
            FlexSpacer::Flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .fixed_height(PREVIEW_HEIGHT.px())
    .background_color(state.palette().app_background)
    .corner_radius(8.0.px())
    .padding(24.0.px())
    .boxed()
}
