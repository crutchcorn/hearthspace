use hearthspace::settings::{Settings, client_get_settings, client_set_test, config_path};
use xilem::{
    EventLoop, WidgetView, WindowOptions, Xilem,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{checkbox, flex_col, label, sized_box},
};

struct SettingsApp {
    settings: Settings,
    status: String,
    config_label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = initial_state();
    let app = Xilem::new_simple(
        state,
        app_logic,
        WindowOptions::new("Hearthspace Settings")
            .with_initial_inner_size(LogicalSize::new(420.0, 220.0))
            .with_min_inner_size(LogicalSize::new(320.0, 180.0)),
    );

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn initial_state() -> SettingsApp {
    let (settings, status) = match client_get_settings() {
        Ok(settings) => (settings, "Connected to hearthspace-settingsd".to_string()),
        Err(error) => (
            Settings::default(),
            format!("Settings backend unavailable: {error}"),
        ),
    };

    let config_label = match config_path() {
        Ok(path) => format!("Config: {}", path.display()),
        Err(error) => format!("Config path unavailable: {error}"),
    };

    SettingsApp {
        settings,
        status,
        config_label,
    }
}

fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let test_checked = state.settings.test;

    sized_box(
        flex_col((
            label("Hearthspace Settings")
                .font(GenericFamily::SystemUi)
                .text_size(22.0),
            label("This is a minimal scaffold for future settings."),
            checkbox("test", test_checked, |state: &mut SettingsApp, checked| {
                let previous = state.settings.test;
                state.settings.test = checked;

                match client_set_test(checked) {
                    Ok(settings) => {
                        state.settings = settings;
                        state.status = format!("Saved test = {}", settings.test);
                    }
                    Err(error) => {
                        state.settings.test = previous;
                        state.status = format!("Failed to save setting: {error}");
                    }
                }
            }),
            label(state.config_label.clone()),
            label(state.status.clone()),
        ))
        .gap(12.0.px()),
    )
    .padding(20.0.px())
}
