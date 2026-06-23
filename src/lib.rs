pub mod compositor;
pub mod config;
pub mod controls;
pub mod geometry;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    compositor::run_winit()
}
