//! Real map clients exercise the backdrop adapter, damage and configure paths.
use std::collections::BTreeSet;

use rstest::rstest;

use super::*;

const WHITE: [u8; 4] = [255; 4];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const SOURCE_SIZE: (i32, i32) = (1000, 900);
const EDGE: usize = 612;

// Keep the numeric readback approach used by shell_blur; its helpers are private
// and its short transition range deliberately measures the smaller corner blur.
struct Pixels {
    data: Vec<u8>,
    width: usize,
    channels: usize,
    scale: usize,
}

impl Pixels {
    fn capture(compositor: &HeadlessCompositor, scale: usize) -> Self {
        let mut reader = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
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

    fn assert_solid(&self, x: usize, y: usize, expected: u8) {
        let actual = self.logical_rgb(x, y);
        assert!(
            actual.iter().all(|channel| channel.abs_diff(expected) <= 3),
            "({x}, {y}) at scale {}: {actual:?}, expected {expected}",
            self.scale
        );
    }

    fn transition(&self, edge: usize, y: usize) -> (usize, usize) {
        let values: Vec<_> = ((edge - 100) * self.scale..(edge + 100) * self.scale)
            .map(|x| self.rgb(x, y * self.scale)[0])
            .filter(|value| (10..245).contains(value))
            .collect();
        let levels = values.iter().copied().collect::<BTreeSet<_>>().len();
        (values.len(), levels)
    }

    fn assert_map_blur(&self, edge: usize, y: usize) {
        let (width, levels) = self.transition(edge, y);
        // A sigma-40 Gaussian spreads this 10..245 transition over about 140
        // logical pixels; the old sigma-6 corner blur produces about 20.
        assert!(
            (100 * self.scale..=170 * self.scale).contains(&width) && levels >= 32,
            "map needs the 40px blur: {width} physical transition pixels, {levels} levels, scale {}",
            self.scale
        );
        let white_side = self.logical_rgb(edge - 32, y)[0];
        let black_side = self.logical_rgb(edge + 32, y)[0];
        assert!(
            (160..235).contains(&white_side) && (20..95).contains(&black_side),
            "blur must reach both sides 32 logical pixels from the edge: {white_side}, {black_side}"
        );
        self.assert_solid(edge - 128, y, 255);
        self.assert_solid(edge + 128, y, 0);
    }
}

fn repaint_edge(client: &mut MapClient, source: usize, edge: usize) {
    client.repaint_rectangle(
        source,
        WHITE,
        (edge as i32, 0, SOURCE_SIZE.0 - edge as i32, SOURCE_SIZE.1),
        BLACK,
    );
}

#[rstest]
#[case::base_id(1, false)]
#[case::qualified_id_scale_two(2, true)]
fn map_glass_blurs_live_shm_content_with_a_rounded_mask_and_cleans_up_on_close(
    #[case] scale: usize,
    #[case] qualified: bool,
) {
    let _guard = headless_test_guard();
    let size = format!("{}x{}", 800 * scale, 600 * scale);
    let mut compositor = HeadlessCompositor::spawn_with_options(&size, None, None, scale as i32);
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let source = client.add(
        "dev.hearthspace.MapGlassSource".into(),
        SOURCE_SIZE,
        (0, 0),
        WHITE,
    );
    let source_id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    // One opaque SHM app supplies both halves. Move its decorations beyond the
    // sampling halo, then pin the viewport so no automatic placement affects it.
    action(&compositor, format!("move-window {source_id} 0 -200"));
    action(&compositor, "pan-to 0 0");
    action(&compositor, "pointer-motion-abs 10 550");
    repaint_edge(&mut client, source, EDGE);
    let before = Pixels::capture(&compositor, scale);
    let y = 234;
    before.assert_solid(EDGE - 32, y, 255);
    before.assert_solid(EDGE + 32, y, 0);
    assert!(
        before.transition(EDGE, y).0 <= 2 * scale,
        "SHM edge must start sharp"
    );

    let app_id = if qualified {
        canvas_map_app_id_for_output(&output)
    } else {
        CANVAS_MAP_APP_ID.into()
    };
    // Nonzero geometry insets ensure the mask follows xdg geometry, rather
    // than including the transparent padding in the client's surface buffer.
    let map = client.add(app_id, (352, 336), (5, 7), [0; 4]);
    assert_eq!(client.size(map), (352, 336));
    let bounds = map_bounds(&compositor);
    assert_eq!(bounds["x"], 436);
    assert_eq!(bounds["y"], 66);
    assert_eq!(bounds["width"], 352);
    assert_eq!(bounds["height"], 336);
    let blurred = Pixels::capture(&compositor, scale);
    blurred.assert_map_blur(EDGE, y);
    assert!(client.closed().is_empty());

    // Nothing moves: a new buffer commit alone must refresh the live cache.
    client.repaint_color(source, WHITE);
    let repainted = Pixels::capture(&compositor, scale);
    for x in [EDGE - 32, EDGE, EDGE + 32] {
        repainted.assert_solid(x, y, 255);
    }
    assert_eq!(map_bounds(&compositor), bounds);

    let left = bounds["x"].as_u64().unwrap() as usize;
    let top = bounds["y"].as_u64().unwrap() as usize;
    let bottom = top + bounds["height"].as_u64().unwrap() as usize;
    repaint_edge(&mut client, source, left - 2);
    let rounded = Pixels::capture(&compositor, scale);
    // Both left vertices now have black source beside white. Allow the panel
    // shadow's small tint over black; a rectangular blur would import far more
    // white here. Nearby pixels inside the radius must still receive that blur.
    for corner_y in [top + 2, bottom - 3] {
        let actual = rounded.logical_rgb(left + 2, corner_y);
        assert!(
            actual.iter().all(|channel| *channel <= 40),
            "outside rounded glass allows shadow tint, not blurred white: {actual:?} at ({}, {corner_y})",
            left + 2
        );
    }
    for inside_y in [top + 8, bottom - 9] {
        let actual = rounded.logical_rgb(left + 8, inside_y);
        assert!(
            actual.iter().all(|channel| (60..160).contains(channel)),
            "radius-20 interior must contain blurred content: {actual:?} at ({}, {inside_y})",
            left + 8
        );
    }

    repaint_edge(&mut client, source, EDGE);
    Pixels::capture(&compositor, scale).assert_map_blur(EDGE, y);
    action(&compositor, "key-down 1");
    assert!(client.closed().contains(&map));
    client.destroy(map);
    action(&compositor, "key-up 1");
    assert_eq!(query(&compositor, "status")["canvas_maps"], json!([]));
    let closed = Pixels::capture(&compositor, scale);
    assert!(
        closed.transition(EDGE, y).0 <= 2 * scale,
        "closing the map must remove its cached blur"
    );
    for x in [EDGE - 32, EDGE - 1, EDGE, EDGE + 32] {
        assert_eq!(closed.logical_rgb(x, y), before.logical_rgb(x, y));
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[rstest]
#[case::narrow(1)]
#[case::narrow_scale_two(2)]
fn map_configures_352_to_292_and_retains_bounds_inside_a_320_logical_output(#[case] scale: i32) {
    let _guard = headless_test_guard();
    let size = format!("{}x{}", 320 * scale, 480 * scale);
    let mut compositor = HeadlessCompositor::spawn_with_options(&size, None, None, scale);
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (352, 336),
        (5, 7),
        [0; 4],
    );
    assert_eq!(
        client.size(map),
        (292, 336),
        "real xdg configure must clamp the client"
    );
    let bounds = json!({"output": output, "x": 16, "y": 66, "width": 292, "height": 336});
    assert_eq!(map_bounds(&compositor), bounds);
    client.repaint_color(map, [0; 4]);
    action(&compositor, "pan-to 500 -300");
    assert_eq!(
        map_bounds(&compositor),
        bounds,
        "a later commit and pan retain screen bounds"
    );

    client.resize(map, (352, 200));
    assert_eq!(client.size(map), (292, 200));
    assert_eq!(
        map_bounds(&compositor),
        json!({"output": output, "x": 16, "y": 66, "width": 292, "height": 200})
    );
    assert!(client.closed().is_empty());
    client.destroy(map);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
