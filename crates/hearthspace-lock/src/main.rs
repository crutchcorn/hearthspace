//! The lock surface is an independent client; only PAM success may unlock it.
mod auth;
mod background;
mod config;
mod session;
mod theme;
mod ui;

fn main() -> gtk::glib::ExitCode {
    // gtk4-session-lock must initialize before GTK realizes a Wayland surface.
    session::run(config::Config::load())
}
