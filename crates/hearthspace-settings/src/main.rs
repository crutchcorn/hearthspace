mod actions;
mod monitor_layout_view;
mod monitor_layout_widget;
mod monitors;
mod settings_backend;
mod state;
mod ui;
mod wayland_outputs;

use state::initial_state;
use ui::app_logic;
use xilem::{EventLoop, WindowOptions, Xilem, dpi::LogicalSize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = initial_state();
    let app = Xilem::new_simple(
        state,
        app_logic,
        WindowOptions::new("Hearthspace Settings")
            .with_initial_inner_size(LogicalSize::new(840.0, 720.0))
            .with_min_inner_size(LogicalSize::new(680.0, 600.0)),
    );

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
