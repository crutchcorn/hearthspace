//! Mask-only virtual keyboard requests must update hover without stealing a drag.
use super::*;
use crate::remote_input::client::RemoteClient;

const LOGO: u32 = 1 << 6; // Mod4 in the explicitly supplied pc keymap.
const SHIFT: u32 = 1;
const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes { include "evdev+aliases(qwerty)" };
    xkb_types { include "complete" };
    xkb_compatibility { include "complete" };
    xkb_symbols { include "pc+us+inet(evdev)" };
};"#;

fn cursor_pixels(compositor: &HeadlessCompositor) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let width = info.width as usize;
    let channels = info.color_type.samples();
    // All named cursor hotspots at (500, 500) fit in this otherwise empty crop.
    // Compare rendered shapes from this run, without a stored screenshot golden.
    (476..540)
        .flat_map(|y| {
            let start = (y * width + 476) * channels;
            pixels[start..start + 64 * channels].iter().copied()
        })
        .collect()
}

#[test]
fn remote_meta_masks_refresh_stationary_canvas_cursor_without_key_events() {
    let _guard = headless_test_guard();
    let compositor = fixture(true);
    let mut client = clients(&compositor);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.set_keymap(KEYMAP);
    remote.motion(500, 500, 1280, 800);
    client.take_pointer_events();
    let default = cursor_pixels(&compositor);
    action(&compositor, "canvas-tool pan");
    let grab = cursor_pixels(&compositor);
    action(&compositor, "canvas-tool select");
    assert!(
        default != grab,
        "fixture must distinguish the rendered cursors"
    );
    assert!(cursor_pixels(&compositor) == default);
    client.take_keyboard_events();

    // No pointer motion or key event may reconcile these modifier-only edges.
    remote.modifiers(LOGO);
    assert!(
        cursor_pixels(&compositor) == grab,
        "Logo mask did not show Grab"
    );
    remote.modifiers(0);
    assert!(
        cursor_pixels(&compositor) == default,
        "clearing Logo left stale Grab"
    );
    remote.modifiers(LOGO);
    remote.modifiers(LOGO | SHIFT);
    assert!(
        cursor_pixels(&compositor) == default,
        "Shift left an ineligible Grab"
    );
    remote.modifiers(LOGO);
    assert!(cursor_pixels(&compositor) == grab);
    // A different virtual keyboard's zero mask can clear Logo during source
    // activation, before its own mask update. That edge needs the same refresh.
    let mut other = RemoteClient::connect(&socket(&compositor));
    other.set_keymap(KEYMAP);
    other.modifiers(0);
    assert!(
        cursor_pixels(&compositor) == default,
        "source change left stale Grab"
    );
    remote.modifiers(LOGO);
    assert!(cursor_pixels(&compositor) == grab);
    remote.modifiers(0);
    assert!(cursor_pixels(&compositor) == default);
    assert_eq!(tool(&compositor), "select");
    assert_eq!(offset(&compositor), [0, 0]);
    assert_silent(&mut client, "mask-only hover sent app pointer input");
    assert!(client.take_keyboard_events().is_empty());
}

#[test]
fn clearing_remote_meta_mask_preserves_grabbing_cursor_and_pointer_source() {
    let _guard = headless_test_guard();
    let compositor = fixture(true);
    let mut client = clients(&compositor);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.set_keymap(KEYMAP);
    remote.motion(500, 500, 1280, 800);
    remote.modifiers(LOGO);
    let grab = cursor_pixels(&compositor);
    remote.button(true);
    let grabbing = cursor_pixels(&compositor);
    assert!(grabbing != grab, "admission must show Grabbing");
    remote.modifiers(0);
    assert!(
        cursor_pixels(&compositor) == grabbing,
        "mask clear ended the drag cursor"
    );
    motion(&compositor, (700., 700.));
    assert_eq!(
        offset(&compositor),
        [0, 0],
        "local motion stole the remote pan"
    );
    remote.motion(550, 550, 1280, 800);
    assert_eq!(
        offset(&compositor),
        [-50, -50],
        "mask clear cancelled the held pan"
    );
    remote.button(false);
    remote.motion(600, 600, 1280, 800);
    assert_eq!(offset(&compositor), [-50, -50]);
    assert_eq!(tool(&compositor), "select");
    assert_silent(&mut client, "remote pan leaked client input");
}
