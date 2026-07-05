use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{
        CrossAxisAlignment, FlexSpacer, checkbox, flex_col, flex_item, flex_row, sized_box,
        text_input,
    },
};
use xilem_storybook::{Control, Story, StorybookState};

const PREVIEW_HEIGHT: f64 = 260.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Xilem::new_simple(
        initial_state(),
        app_logic,
        WindowOptions::new("Hearthspace UI Storybook")
            .with_initial_inner_size(LogicalSize::new(1100.0, 760.0))
            .with_min_inner_size(LogicalSize::new(820.0, 600.0)),
    )
    .with_default_properties(components::hearthspace_light_properties())
    .with_default_base_color(components::APP_BACKGROUND);

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn initial_state() -> StorybookState {
    StorybookState::new(vec![
        Story::new(
            "page-title",
            "Page Title",
            "Large section heading used at the top of settings-style surfaces.",
            vec![Control::text("text", "Text", "Appearance")],
        ),
        Story::new(
            "section-title",
            "Section Title",
            "Compact heading for grouped controls inside a section.",
            vec![Control::text("text", "Text", "Canvas Background")],
        ),
        Story::new(
            "muted-text",
            "Muted Text",
            "Secondary text for descriptions, metadata, and status details.",
            vec![Control::text(
                "text",
                "Text",
                "Connected to hearthspace-settingsd",
            )],
        ),
        Story::new(
            "action-button",
            "Action Button",
            "Default command button for settings actions.",
            vec![Control::text("text", "Text", "Select background image")],
        ),
        Story::new(
            "compact-action-button",
            "Compact Action Button",
            "Small command button used by dense shell surfaces.",
            vec![Control::text("text", "Text", "Zoom In")],
        ),
        Story::new(
            "segmented-option",
            "Segmented Option",
            "Option button for mutually exclusive setting choices.",
            vec![
                Control::text("text", "Text", "Cover"),
                Control::bool("selected", "Selected", true),
            ],
        ),
        Story::new(
            "sidebar-item",
            "Sidebar Item",
            "Navigation row for category sidebars.",
            vec![
                Control::text("text", "Text", "Appearance"),
                Control::bool("selected", "Selected", true),
            ],
        ),
        Story::new(
            "settings-section",
            "Settings Section",
            "Framed settings group with a title and body content.",
            vec![
                Control::text("title", "Title", "Canvas Background"),
                Control::text(
                    "body",
                    "Body",
                    "Background fit controls and image selection.",
                ),
            ],
        ),
        Story::new(
            "sidebar-panel",
            "Sidebar Panel",
            "Surface used to contain settings navigation.",
            vec![Control::text("label", "Selected label", "Display")],
        ),
        Story::new(
            "launcher-item",
            "Launcher Item",
            "Search result row for the Hearthspace shell launcher.",
            vec![Control::text("text", "Text", "Hearthspace Settings")],
        ),
    ])
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
    let story = state.selected_story();
    let content = match story.id() {
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
    .background_color(components::SURFACE_SUBTLE)
    .corner_radius(8.0.px())
    .padding(24.0.px())
    .boxed()
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
            let input = text_input(
                text_control.value().to_string(),
                move |state: &mut StorybookState, value| {
                    state.set_selected_text(&id, value);
                },
            )
            .font(GenericFamily::SystemUi);

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
            checkbox(
                control.label().to_string(),
                bool_control.value(),
                move |state: &mut StorybookState, checked| {
                    state.set_selected_bool(&id, checked);
                },
            )
            .boxed()
        }
    }
}

fn text(state: &StorybookState, id: &str) -> String {
    state.selected_text(id).unwrap_or_default().to_string()
}

fn bool_value(state: &StorybookState, id: &str) -> bool {
    state.selected_bool(id).unwrap_or(false)
}
