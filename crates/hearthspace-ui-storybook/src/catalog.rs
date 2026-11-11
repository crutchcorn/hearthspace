use xilem_storybook::{Control, Story, StorybookState};
pub(super) fn initial_state() -> StorybookState {
    StorybookState::new(vec![
        Story::new(
            "settings-materials",
            "Settings Materials",
            "Reference glass highlights, orange glow, and native button interaction states.",
            vec![
                Control::bool("selected", "Selected", false),
                Control::bool("disabled", "Disabled", false),
            ],
        ),
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
