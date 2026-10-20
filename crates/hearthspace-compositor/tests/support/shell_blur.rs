//! Real SHM content must blur through corner glass and stay live as apps change.
//! Numeric edge-spread checks avoid golden images and font/theme dependencies.
use std::collections::BTreeSet;

use hearthspace_common::{ShellCorner, shell_corner_app_id};
use rstest::rstest;
use serde_json::Value;

use super::navigation::{action, query};
use super::shell_panels::connect;
use super::*;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

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
    client.add("dev.hearthspace.BlurBase".into(), (380, 220), (0, 0), WHITE);
    let foreground = client.add("dev.hearthspace.BlurEdge".into(), (240, 220), (0, 0), BLACK);
    let base_id = app_id(&compositor, "dev.hearthspace.BlurBase");
    let edge_id = app_id(&compositor, "dev.hearthspace.BlurEdge");
    // Keep both clients' titlebars and other content edges far from the glass.
    action(&compositor, format!("move-window {base_id} 564 420"));
    action(&compositor, format!("move-window {edge_id} 708 420"));
    action(&compositor, "pointer-motion-abs 10 100");
    client.sync();
    let before = Pixels::capture(&compositor, scale, "blur-source.png");
    let y = 564;
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
        (160, 48),
        (5, 7),
        [0; 4],
    );
    let bounds = query(&compositor, "status")["shell_corners"][0].clone();
    assert_eq!(bounds["x"], 628);
    assert_eq!(bounds["y"], 540);
    let blurred = Pixels::capture(&compositor, scale, "blur-initial.png");
    blurred.assert_blurred_edge(708, y);
    assert_eq!(blurred.logical_rgb(708, 538), before.logical_rgb(708, 538));

    // Put contrast beside the rounded vertices. A square blur mask would now
    // alter these pixels, while both original source vertices remain white.
    action(&compositor, format!("move-window {edge_id} 630 420"));
    client.sync();
    let rounded = Pixels::capture(&compositor, scale, "blur-rounded-edge.png");
    rounded.assert_blurred_edge(630, y);
    rounded.assert_rounded_outside_unchanged(&before, &bounds);

    action(&compositor, format!("move-window {edge_id} 732 420"));
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
    client.repaint_color(foreground, BLACK);
    client.resize(corner, (200, 48));
    let resized = Pixels::capture(&compositor, scale, "blur-resized-corner.png");
    resized.assert_blurred_edge(732, y);
    let bounds = query(&compositor, "status")["shell_corners"][0].clone();
    assert_eq!(bounds["x"], 588);
    assert_eq!(bounds["width"], 200);

    // Exercise the other shader axis and vertical texture orientation. Place
    // the opaque client's bottom edge through the glass, accounting for SSD.
    let height = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == edge_id)
        .unwrap()["height"]
        .as_i64()
        .unwrap();
    action(
        &compositor,
        format!("move-window {edge_id} 564 {}", 564 - height),
    );
    client.sync();
    let horizontal = Pixels::capture(&compositor, scale, "blur-horizontal-edge.png");
    horizontal.assert_blurred_horizontal_edge(564, 708);
    action(
        &compositor,
        format!("move-window {edge_id} 564 {}", 540 - height),
    );
    client.sync();
    let vertical_motion = Pixels::capture(&compositor, scale, "blur-vertical-motion.png");
    vertical_motion.assert_blurred_horizontal_edge(540, 708);
    vertical_motion.assert_solid(708, 564, 255, "vertical motion exposes fresh white content");

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
