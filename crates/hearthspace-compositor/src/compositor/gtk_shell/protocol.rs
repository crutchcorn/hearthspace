//! Bindings for the unmodified upstream GTK protocol; advertise only version 5.
#![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
#![allow(non_upper_case_globals, non_snake_case, unused_imports, clippy::all)]

use wayland_server;
use wayland_server::protocol::*;

mod __interfaces {
    use wayland_server::backend as wayland_backend;
    use wayland_server::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/gtk-shell.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_server_code!("protocols/gtk-shell.xml");
