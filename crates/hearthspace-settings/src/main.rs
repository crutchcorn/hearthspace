mod actions;
mod ai;
mod ai_view;
mod ai_worker;
mod appearance_view;
mod background_view;
mod cli;
mod display_view;
mod favorites;
mod favorites_search;
mod favorites_view;
mod monitor_layout_view;
mod monitor_layout_widget;
mod monitors;
mod page_components;
mod settings_backend;
mod state;
mod system_view;
mod ui;
mod wayland_outputs;
mod window_frame;

#[cfg(test)]
#[path = "favorites_view/test_support.rs"]
mod test_support;

use state::initial_state;
use ui::app_logic;
use xilem::{EventLoop, WindowOptions, WindowOptionsExtLinux, Xilem, dpi::LogicalSize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let category = match cli::parse(std::env::args_os().skip(1))? {
        cli::Startup::Page(category) => category,
        cli::Startup::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
    };
    let state = initial_state(category);
    let app = Xilem::new_simple(
        state,
        app_logic,
        WindowOptions::new("Hearthspace Settings")
            .with_name("dev.hearthspace.Settings", "dev.hearthspace.Settings")
            .with_transparent(true)
            .with_initial_inner_size(LogicalSize::new(680.0, 490.0))
            .with_min_inner_size(LogicalSize::new(620.0, 420.0)),
    )
    .with_default_base_color(xilem::Color::TRANSPARENT)
    .with_default_properties(hearthspace_ui_components::hearthspace_light_properties());

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
