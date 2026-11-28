//! Real frame surfaces share glass sampling and fullscreen visibility.
use super::*;
use crate::frame_widgets;
use hearthspace_common::frame_layout::frame_group_app_id;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

const APP: [u8; 4] = [190, 70, 20, 255];
const ITEM: [u8; 4] = [20, 190, 70, 255];
const ITEM_ID: &str = "file:6672616d652d6d6174657269616c";

fn pixels(compositor: &HeadlessCompositor) -> Vec<u8> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut data = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut data).unwrap();
    let stride = info.color_type.samples();
    // Interior of the centered bottom backing, crossing the background's edge.
    (750..770)
        .flat_map(|y| (630..650).map(move |x| (y * info.width as usize + x) * stride))
        .map(|offset| data[offset])
        .collect()
}
fn place(compositor: &HeadlessCompositor, target: Value, placement: Value) {
    action(compositor, "layout-tool");
    command(compositor, "place", target, placement);
    command(compositor, "done", Value::Null, Value::Null);
}
fn center() -> Value {
    json!({"surface":"frame","edge":"bottom","position":0.5})
}

#[rstest::rstest]
#[case::shortcut(true)]
#[case::live_window(false)]
fn frame_layout_pinned_apps_hide_and_reject_input_until_fullscreen_ends(#[case] shortcut: bool) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let app = client.add_decorated(
        "frame-fullscreen-app".into(),
        (1280, 800),
        APP,
        Mode::ClientSide,
    );
    let id = window(&compositor, "frame-fullscreen-app")["id"]
        .as_u64()
        .unwrap();
    let (item, target) = if shortcut {
        (
            client.add(
                hearthspace_common::desktop_item_app_id(ITEM_ID),
                (100, 108),
                (0, 0),
                ITEM,
            ),
            json!({"kind":"item","id":ITEM_ID}),
        )
    } else {
        let item = client.add_decorated(
            "pinned-live-window".into(),
            (100, 108),
            ITEM,
            Mode::ClientSide,
        );
        let id = window(&compositor, "pinned-live-window")["id"]
            .as_u64()
            .unwrap();
        (item, json!({"kind":"window","id":id.to_string()}))
    };
    place(&compositor, target, center());
    if shortcut {
        client.resize(item, (54, 54));
    }
    let layout_before = layout(&compositor)["outputs"][0]["layout"].clone();
    super::desktop_layers::assert_color(
        &compositor,
        (636, 768, 8, 8),
        ITEM,
        "pinned shortcut must be visible",
    );
    action(&compositor, format!("fullscreen-window {id} true"));
    client.sync();
    super::desktop_layers::assert_color(
        &compositor,
        (636, 768, 8, 8),
        APP,
        "fullscreen must cover the pinned shortcut",
    );
    super::super::shell_panels::click(&compositor, 640., 772.);
    assert_eq!(
        client.presses().last().map(|(number, _)| *number),
        Some(app)
    );
    action(&compositor, format!("fullscreen-window {id} false"));
    client.sync();
    // The capture includes the software cursor; keep it outside the color ROI.
    action(&compositor, "pointer-motion-abs 1000 700");
    super::desktop_layers::assert_color(
        &compositor,
        (636, 768, 8, 8),
        ITEM,
        "shortcut must return after fullscreen",
    );
    if shortcut {
        // Pinned shortcuts activate through the compositor, not the tile client.
        let presses = client.presses().len();
        super::super::shell_panels::click(&compositor, 640., 772.);
        assert_eq!(
            client.presses().len(),
            presses,
            "the restored shortcut must consume the click instead of leaking it to the app"
        );
    }
    assert_eq!(layout(&compositor)["outputs"][0]["layout"], layout_before);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_shortcuts_widgets_and_groups_use_one_matching_blur() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    action(&compositor, "pointer-motion-abs 1000 700");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let background = client.add_decorated(
        "glass-background".into(),
        (1280, 800),
        [0, 0, 0, 255],
        Mode::ClientSide,
    );
    client.repaint_rectangle(background, [0, 0, 0, 255], (0, 0, 640, 800), [255; 4]);
    let id = window(&compositor, "glass-background")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {id} 0 0"));
    let sharp = pixels(&compositor);
    let mut samples = Vec::new();
    for (app_id, target) in [
        (
            hearthspace_common::desktop_item_app_id(ITEM_ID),
            json!({"kind":"item","id":ITEM_ID}),
        ),
        (
            frame_widgets::app_id(&compositor, Some(&output), "tools"),
            frame_widgets::target(&compositor, "tools"),
        ),
        (
            frame_group_app_id(Some(&output), "system"),
            json!({"kind":"group","id":"system"}),
        ),
    ] {
        let members: Vec<_> = if target["kind"] == "group" {
            ["clock", "wifi"]
                .into_iter()
                .map(|id| {
                    client.add(
                        frame_widgets::app_id(&compositor, Some(&output), id),
                        (44, 44),
                        (0, 0),
                        [0; 4],
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let surface = client.add(app_id, (54, 54), (0, 0), [0; 4]);
        place(&compositor, target.clone(), center());
        client.resize(surface, (54, 54));
        samples.push(pixels(&compositor));
        let parked = if target["kind"] == "group" {
            json!({"surface":"hidden"})
        } else {
            json!({"surface":"canvas","x":100.,"y":100.})
        };
        place(&compositor, target, parked);
        client.destroy(surface);
        for member in members {
            client.destroy(member);
        }
    }
    for (kind, sample) in ["shortcut", "toolbar", "group"].into_iter().zip(&samples) {
        assert!(
            sample
                .iter()
                .enumerate()
                .any(|(i, value)| i % 20 >= 10 && *value > 20),
            "{kind} must spread light into the black side of the desktop edge, not just paint a shadow"
        );
        assert_ne!(*sample, sharp, "{kind} must change the sharp desktop");
        assert!(
            sample
                .iter()
                .zip(&samples[0])
                .all(|(a, b)| a.abs_diff(*b) <= 8),
            "{kind} must match shortcut blur without another blur layer"
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
