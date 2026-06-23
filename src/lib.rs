pub mod compositor;
pub mod config;
pub mod geometry;
pub mod shell;
pub mod shell_bar;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    compositor::run_winit()
}
