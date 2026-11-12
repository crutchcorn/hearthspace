use hearthspace_ui_components as components;
use state::AppState as StorybookState;
use xilem::{
    AnyWidgetView, EventLoop, WidgetView, Xilem,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, FlexSpacer, flex_col, flex_item, flex_row, sized_box},
};
use xilem_storybook::Control;
mod catalog;
mod legacy;
#[cfg(test)]
mod preview_tests;
mod shared;
mod state;

const PREVIEW_HEIGHT: f64 = 260.0;

mod settings_materials;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = StorybookState::new();
    let defaults =
        components::hearthspace_properties(state.palette(), components::PropertyStyle::Opaque);
    let id = xilem::WindowId::next();
    let app = Xilem::new(state, move |state: &mut StorybookState| {
        state.update_theme();
        [
            xilem::window(id, "Hearthspace UI Storybook", app_logic(state))
                .with_default_properties(state.properties.clone())
                .with_base_color(state.palette().app_background)
                .with_options(|options| {
                    options
                        .with_initial_inner_size(LogicalSize::new(1100.0, 760.0))
                        .with_min_inner_size(LogicalSize::new(760.0, 600.0))
                        .on_close(|s: &mut StorybookState| s.running = false)
                }),
        ]
        .into_iter()
    })
    .with_default_properties(defaults);
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn app_logic(state: &mut StorybookState) -> impl WidgetView<StorybookState> + use<> {
    components::app_background(
        flex_row((
            sized_box(sidebar(state)).fixed_width(240.0.px()),
            flex_item(main_area(state), 1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(20.0.px()),
    )
}

fn sidebar(state: &StorybookState) -> impl WidgetView<StorybookState> + use<> {
    let selected = state.selected_story_index();
    let entries: Vec<Box<AnyWidgetView<StorybookState>>> = state
        .stories()
        .iter()
        .enumerate()
        .map(|(index, story)| {
            components::sidebar_item(
                story.name().to_string(),
                selected == index,
                move |state: &mut StorybookState| {
                    state.select_story(index);
                },
            )
            .boxed()
        })
        .collect();

    components::sidebar_panel(components::scroll_area(
        flex_col((
            components::page_title("Components"),
            components::muted_text("hearthspace-ui-components"),
            entries,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    ))
}

fn main_area(state: &StorybookState) -> impl WidgetView<StorybookState> + use<> {
    let story = state.selected_story();
    components::scroll_area(
        flex_col((
            shared::theme_controls(state),
            components::page_title(story.name().to_string()),
            components::muted_text(story.description().to_string()),
            components::settings_section("Preview", preview(state)),
            components::settings_section("Controls", controls(state)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(16.0.px()),
    )
}

fn preview(state: &StorybookState) -> Box<AnyWidgetView<StorybookState>> {
    if state.selected_story().id().starts_with("shared-controls-") {
        return shared::preview(state);
    }
    legacy::preview(state)
}

fn controls(state: &StorybookState) -> Box<AnyWidgetView<StorybookState>> {
    let controls: Vec<Box<AnyWidgetView<StorybookState>>> = state
        .selected_story()
        .controls()
        .iter()
        .map(control_row)
        .collect();

    if controls.is_empty() {
        return components::muted_text("This story has no controls.").boxed();
    }

    flex_col(controls)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px())
        .boxed()
}

fn control_row(control: &Control) -> Box<AnyWidgetView<StorybookState>> {
    match control {
        Control::Text(text_control) => {
            let id = control.id().to_string();
            let input = components::controls::text_field(
                text_control.value().to_string(),
                "",
                false,
                false,
                move |state: &mut StorybookState, value| {
                    state.set_selected_text(&id, value);
                },
            );

            flex_col((
                components::field_label(control.label().to_string()),
                sized_box(input).fixed_width(360.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(6.0.px())
            .boxed()
        }
        Control::Bool(bool_control) => {
            let id = control.id().to_string();
            components::controls::labelled_checkbox(
                control.label().to_string(),
                bool_control.value(),
                false,
                move |state: &mut StorybookState, checked| state.set_selected_bool(&id, checked),
            )
        }
    }
}

fn text(state: &StorybookState, id: &str) -> String {
    state.selected_text(id).unwrap_or_default().to_string()
}

fn bool_value(state: &StorybookState, id: &str) -> bool {
    state.selected_bool(id).unwrap_or(false)
}
