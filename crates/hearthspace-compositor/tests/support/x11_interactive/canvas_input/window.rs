//! Colored X11 windows used to distinguish input recipients and popup pixels.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{CreateWindowAux, EventMask, WindowClass};

pub(super) fn window(
    connection: &RustConnection,
    at: [i32; 2],
    size: [u16; 2],
    popup: bool,
) -> Window {
    let screen = &connection.setup().roots[0];
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            window,
            screen.root,
            at[0].try_into().unwrap(),
            at[1].try_into().unwrap(),
            size[0],
            size[1],
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(if popup { 0x20c060 } else { 0x204080 })
                .override_redirect(u32::from(popup))
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )
        .unwrap();
    let motif = connection
        .intern_atom(false, b"_MOTIF_WM_HINTS")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    connection
        .change_property32(PropMode::REPLACE, window, motif, motif, &[2, 0, 0, 0, 0])
        .unwrap();
    window
}
