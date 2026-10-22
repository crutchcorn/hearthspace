//! Real SHM content must blur through corner glass and stay live as apps change.
//! Numeric edge-spread checks avoid golden images and font/theme dependencies.
use std::collections::BTreeSet;

use hearthspace_common::{LAUNCHER_APP_ID, ShellCorner, shell_corner_app_id};
use rstest::rstest;
use serde_json::Value;

use super::navigation::{action, query};
use super::shell_panels::connect;
use super::*;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const SOURCE_SIZE: (i32, i32) = (800, 600);
const CONTENT_EDGE: i32 = 320;
const VERTICAL_RECT: (i32, i32, i32, i32) =
    (CONTENT_EDGE, 0, SOURCE_SIZE.0 - CONTENT_EDGE, SOURCE_SIZE.1);

struct Pixels {
    data: Vec<u8>,
    width: usize,
    channels: usize,
    scale: usize,
}

impl Pixels {
    fn capture(compositor: &HeadlessCompositor, scale: usize, name: &str) -> Self {
        let png = compositor.take_screenshot();
        std::fs::write(compositor.runtime_dir.path().join(name), &png).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        Self {
            data,
            width: info.width as usize,
            channels: info.color_type.samples(),
            scale,
        }
    }

    fn rgb(&self, x: usize, y: usize) -> [u8; 3] {
        let offset = (y * self.width + x) * self.channels;
        self.data[offset..offset + 3].try_into().unwrap()
    }

    fn logical_rgb(&self, x: usize, y: usize) -> [u8; 3] {
        self.rgb(x * self.scale, y * self.scale)
    }

    fn assert_solid(&self, x: usize, y: usize, expected: u8, message: &str) {
        let actual = self.logical_rgb(x, y);
        assert!(
            actual.iter().all(|channel| channel.abs_diff(expected) <= 3),
            "{message}: ({x}, {y}) is {actual:?}, expected {expected}"
        );
    }

    fn transition(&self, edge: usize, y: usize) -> (usize, usize) {
        let values: Vec<_> = ((edge - 12) * self.scale..(edge + 12) * self.scale)
            .map(|x| self.rgb(x, y * self.scale)[0])
            .filter(|&value| (10..245).contains(&value))
            .collect();
        let levels = values.iter().copied().collect::<BTreeSet<_>>().len();
        (values.len(), levels)
    }

    fn assert_blurred_edge(&self, edge: usize, y: usize) {
        let (width, levels) = self.transition(edge, y);
        assert!(
            width >= 4 * self.scale && levels >= 4,
            "a sharp client edge needs a visible blur transition, got {width} physical pixels and {levels} levels at scale {}",
            self.scale
        );
        self.assert_solid(
            edge - 28,
            y,
            255,
            "white source survives beyond blur radius",
        );
        self.assert_solid(edge + 28, y, 0, "black source survives beyond blur radius");
    }

    fn assert_blurred_horizontal_edge(&self, edge: usize, x: usize) {
        let values: Vec<_> = ((edge - 12) * self.scale..(edge + 12) * self.scale)
            .map(|y| self.rgb(x * self.scale, y)[0])
            .filter(|&value| (10..245).contains(&value))
            .collect();
        let levels = values.iter().copied().collect::<BTreeSet<_>>().len();
        assert!(
            values.len() >= 4 * self.scale && levels >= 4,
            "horizontal client edge needs vertical blur, got {} physical pixels and {levels} levels",
            values.len()
        );
        self.assert_solid(x, edge - 28, 0, "black source above horizontal edge");
        self.assert_solid(x, edge + 28, 255, "white source below horizontal edge");
    }

    fn assert_rounded_outside_unchanged(&self, before: &Self, bounds: &Value) {
        let number = |key| bounds[key].as_u64().unwrap() as usize * self.scale;
        let (left, top) = (number("x"), number("y"));
        let (right, bottom) = (left + number("width"), top + number("height"));
        for x in [left, right - 1] {
            for y in [top, bottom - 1] {
                assert_eq!(
                    self.rgb(x, y),
                    before.rgb(x, y),
                    "rounded glass must preserve backdrop at exterior vertex ({x}, {y})"
                );
            }
        }
    }
}

fn app_id(compositor: &HeadlessCompositor, app: &str) -> u64 {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["app_id"] == app)
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[rstest]
#[case(1)]
#[case(2)]
fn glass_blurs_real_window_edges_and_refreshes_after_motion_and_content_commits(
    #[case] scale: usize,
) {
    let _guard = headless_test_guard();
    let size = format!("{}x{}", 800 * scale, 600 * scale);
    let mut compositor = HeadlessCompositor::spawn_with_options(&size, None, None, scale as i32);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    // Keep the base partly outside the foreground's bounds. Canvas avoidance
    // uses window rectangles even when their SHM pixels are transparent.
    let base = client.add("dev.hearthspace.BlurBase".into(), (380, 800), (0, 0), WHITE);
    let foreground = client.add(
        "dev.hearthspace.BlurEdge".into(),
        SOURCE_SIZE,
        (0, 0),
        [0; 4],
    );
    client.repaint_rectangle(foreground, [0; 4], VERTICAL_RECT, BLACK);
    let base_id = app_id(&compositor, "dev.hearthspace.BlurBase");
    let edge_id = app_id(&compositor, "dev.hearthspace.BlurEdge");
    // Test an internal SHM edge, with the white app visible through the rest
    // of the foreground. Its frame, shadow and inset glow stay far away, so
    // strict source/blur pixel assertions do not depend on decoration styling.
    action(&compositor, format!("move-window {base_id} 564 420"));
    action(
        &compositor,
        format!("move-window {edge_id} {} 420", 708 - CONTENT_EDGE),
    );
    action(&compositor, "pointer-motion-abs 10 100");
    client.sync();
    let before = Pixels::capture(&compositor, scale, "blur-source.png");
    let y = 549;
    before.assert_solid(680, y, 255, "real white window content");
    before.assert_solid(736, y, 0, "real black window content");
    assert!(
        before.transition(708, y).0 <= 2 * scale,
        "source edge must be sharp"
    );

    // Transparent pixels isolate compositor blur from any shell tint. A nonzero
    // xdg geometry inset catches accidentally blurring the entire surface buffer.
    let corner = client.add(
        shell_corner_app_id(ShellCorner::Apps, None),
        (160, 54),
        (5, 7),
        [0; 4],
    );
    let bounds = query(&compositor, "status")["shell_corners"][0].clone();
    assert_eq!(bounds["x"], 616);
    assert_eq!(bounds["y"], 522);
    let blurred = Pixels::capture(&compositor, scale, "blur-initial.png");
    blurred.assert_blurred_edge(708, y);
    assert_eq!(blurred.logical_rgb(708, 520), before.logical_rgb(708, 520));

    // Put contrast beside the rounded vertices. A square blur mask would now
    // alter these pixels, while both original source vertices remain white.
    action(
        &compositor,
        format!("move-window {edge_id} {} 420", 618 - CONTENT_EDGE),
    );
    client.sync();
    let rounded = Pixels::capture(&compositor, scale, "blur-rounded-edge.png");
    rounded.assert_blurred_edge(618, y);
    rounded.assert_rounded_outside_unchanged(&before, &bounds);

    action(
        &compositor,
        format!("move-window {edge_id} {} 420", 732 - CONTENT_EDGE),
    );
    client.sync();
    let moved = Pixels::capture(&compositor, scale, "blur-moved-window.png");
    moved.assert_blurred_edge(732, y);
    moved.assert_solid(708, y, 255, "blur must not retain the window's old edge");

    // No geometry changes: only a new SHM content commit can invalidate this.
    client.repaint_color(foreground, WHITE);
    let repainted = Pixels::capture(&compositor, scale, "blur-repainted-window.png");
    for x in [704, 720, 732, 744, 760] {
        repainted.assert_solid(x, y, 255, "blur must refresh after a client repaint");
    }

    // Resizing a right-anchored corner must move its mask and cache allocation.
    client.repaint_rectangle(foreground, [0; 4], VERTICAL_RECT, BLACK);
    client.resize(corner, (200, 54));
    let resized = Pixels::capture(&compositor, scale, "blur-resized-corner.png");
    resized.assert_blurred_edge(732, y);
    let bounds = query(&compositor, "status")["shell_corners"][0].clone();
    assert_eq!(bounds["x"], 576);
    assert_eq!(bounds["width"], 200);

    // Exercise the other shader axis and vertical texture orientation using an
    // internal rectangle bottom, still distant from the actual window frame.
    let height = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == edge_id)
        .unwrap()["height"]
        .as_i64()
        .unwrap();
    let header = height - i64::from(SOURCE_SIZE.1);
    let internal_bottom = 240;
    client.repaint_rectangle(
        foreground,
        [0; 4],
        (0, 0, SOURCE_SIZE.0, internal_bottom),
        BLACK,
    );
    action(
        &compositor,
        format!(
            "move-window {edge_id} 300 {}",
            549 - header - i64::from(internal_bottom)
        ),
    );
    client.sync();
    let horizontal = Pixels::capture(&compositor, scale, "blur-horizontal-edge.png");
    horizontal.assert_blurred_horizontal_edge(549, 708);
    action(
        &compositor,
        format!(
            "move-window {edge_id} 300 {}",
            525 - header - i64::from(internal_bottom)
        ),
    );
    client.sync();
    let vertical_motion = Pixels::capture(&compositor, scale, "blur-vertical-motion.png");
    vertical_motion.assert_blurred_horizontal_edge(525, 708);
    vertical_motion.assert_solid(708, 549, 255, "vertical motion exposes fresh white content");

    // Palettes share the glass renderer, but have different placement and much
    // taller geometry than a corner. Exercise real content beneath the launcher.
    client.repaint_with_size(base, (800, 600));
    client.repaint_rectangle(foreground, [0; 4], VERTICAL_RECT, BLACK);
    action(&compositor, format!("move-window {base_id} 0 0"));
    action(
        &compositor,
        format!("move-window {edge_id} {} 0", 650 - CONTENT_EDGE),
    );
    client.sync();
    let panel_source = Pixels::capture(&compositor, scale, "blur-launcher-source.png");
    assert!(panel_source.transition(650, 400).0 <= 2 * scale);
    let panel = client.add(LAUNCHER_APP_ID.into(), (300, 200), (6, 9), [0; 4]);
    let panel_bounds = query(&compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|panel| panel["kind"] == "search")
        .unwrap()
        .clone();
    let panel_y = panel_bounds["y"].as_u64().unwrap() as usize;
    let panel_x = panel_bounds["x"].as_u64().unwrap() as usize;
    assert_eq!(panel_bounds["width"], 300);
    assert_eq!(panel_bounds["height"], 200);
    assert!(panel_y + 200 < 522, "launcher must sit above the app dock");
    let panel_pixels = Pixels::capture(&compositor, scale, "blur-launcher-edge.png");
    panel_pixels.assert_blurred_edge(650, panel_y + 100);
    panel_pixels.assert_rounded_outside_unchanged(&panel_source, &panel_bounds);
    assert_eq!(
        panel_pixels.logical_rgb(650, panel_y - 2),
        panel_source.logical_rgb(650, panel_y - 2)
    );
    action(
        &compositor,
        format!(
            "move-window {edge_id} {} 0",
            panel_x as i32 + 2 - CONTENT_EDGE
        ),
    );
    client.sync();
    let panel_moved = Pixels::capture(&compositor, scale, "blur-launcher-moved.png");
    panel_moved.assert_blurred_edge(panel_x + 2, panel_y + 100);
    panel_moved.assert_rounded_outside_unchanged(&panel_source, &panel_bounds);
    client.repaint_color(foreground, WHITE);
    let panel_repainted = Pixels::capture(&compositor, scale, "blur-launcher-repainted.png");
    panel_repainted.assert_solid(
        650,
        panel_y + 100,
        255,
        "launcher blur must update with its backdrop",
    );
    client.destroy(panel);
    client.repaint_color(foreground, BLACK);

    // The cached white half must never leak over a newly fullscreen black app.
    action(&compositor, format!("fullscreen-window {edge_id} true"));
    client.sync();
    assert!(
        query(&compositor, "status")["shell_corners"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let fullscreen = Pixels::capture(&compositor, scale, "blur-hidden-fullscreen.png");
    for x in [600, 680, 732, 780] {
        fullscreen.assert_solid(x, y, 0, "fullscreen must hide cached corner glass");
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
