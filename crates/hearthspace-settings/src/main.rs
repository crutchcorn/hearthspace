mod actions;
mod activation;
mod ai;
mod ai_view;
mod ai_worker;
mod appearance;
mod appearance_view;
mod background_preview;
mod background_thumbnail;
mod background_view;
mod cli;
mod display_view;
mod favorites;
mod favorites_icons;
mod favorites_search;
mod favorites_view;
mod frame_settings;
mod monitor_layout_view;
mod monitor_layout_widget;
mod monitors;
mod page_components;
mod settings_backend;
mod settings_refresh;
mod shortcuts_view;
mod state;
mod system_view;
mod typography;
mod ui;
mod wayland_outputs;
mod window_frame;

#[cfg(test)]
#[path = "favorites_view/test_support.rs"]
mod test_support;

use state::initial_state;
use ui::app_logic;
use xilem::{EventLoop, WindowId, WindowOptionsExtLinux, Xilem, dpi::LogicalSize, window};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let category = match cli::parse(std::env::args_os().skip(1))? {
        cli::Startup::Page(category) => category,
        cli::Startup::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
    };
    let activation = activation::start(category)?;
    if matches!(activation, activation::Startup::Forwarded) {
        return Ok(());
    }
    let mut state = initial_state(category);
    if let activation::Startup::Primary(Some(endpoint)) = &activation {
        state.activation_listener = Some(endpoint.listener.try_clone()?);
    }
    // The pinned WindowView applies per-window defaults during rebuild only.
    // Its first RenderRoot must receive the persisted palette from app defaults.
    let initial_properties = appearance::properties(state.palette());
    let id = WindowId::next();
    let app = Xilem::new(state, move |state: &mut state::SettingsApp| {
        let properties = state.appearance_theme.properties(
            state.settings.appearance,
            state.settings.background_image_path.is_some(),
        );
        [window(id, "Hearthspace Settings", app_logic(state))
            .with_default_properties(properties)
            .with_options(|options| {
                options
                    .with_name("dev.hearthspace.Settings", "dev.hearthspace.Settings")
                    .with_transparent(true)
                    .with_initial_inner_size(LogicalSize::new(680.0, 490.0))
                    .with_min_inner_size(LogicalSize::new(400.0, 420.0))
                    .on_close(|state: &mut state::SettingsApp| state.running = false)
            })]
        .into_iter()
    })
    .with_default_base_color(xilem::Color::TRANSPARENT)
    .with_default_properties(initial_properties);

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
