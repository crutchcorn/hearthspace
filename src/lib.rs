pub mod a11y;
pub mod compositor;
pub mod config;
pub mod geometry;
pub mod gtk_test_app;
pub mod idle;
pub mod shell;
pub mod shell_bar;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub scroll_zooms_without_super: bool,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_with_options(RunOptions::default())
}

pub fn run_with_options(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    compositor::run_winit(options)
}
