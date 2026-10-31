use super::*;
use smithay::reexports::x11rb::protocol::xproto::ConnectionExt;

pub(super) fn geometry(compositor: &HeadlessCompositor, id: u64) -> [i32; 4] {
    let windows = query(compositor, "windows");
    let window = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap();
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap() as i32)
}

fn colored_bounds(compositor: &HeadlessCompositor, color: [u8; 3]) -> Option<[i32; 4]> {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let mut bounds = None;
    for (index, pixel) in data[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .enumerate()
    {
        if pixel[..3] != color {
            continue;
        }
        let (x, y) = (
            (index % info.width as usize) as i32,
            (index / info.width as usize) as i32,
        );
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => {
                (x.min(left), y.min(top), x.max(right), y.max(bottom))
            }
        });
    }
    bounds.map(|(x, y, right, bottom)| [x, y, right - x + 1, bottom - y + 1])
}

pub(super) fn wait_geometry(
    compositor: &HeadlessCompositor,
    client: &mut Client,
    expected: [i32; 4],
    color: [u8; 3],
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        client.pump();
        let actual = geometry(compositor, client.id);
        let origin = client.origin();
        let backing = client
            .connection
            .get_geometry(client.window)
            .unwrap()
            .reply()
            .unwrap();
        let root = client
            .connection
            .get_geometry(client.root)
            .unwrap()
            .reply()
            .unwrap();
        let backing_inside_root = origin.0 >= 0
            && origin.1 >= 0
            && origin.0 + i32::from(backing.width) <= i32::from(root.width)
            && origin.1 + i32::from(backing.height) <= i32::from(root.height);
        let pixels = colored_bounds(compositor, color);
        if actual == expected
            && [i32::from(backing.width), i32::from(backing.height)] == [expected[2], expected[3]]
            && backing_inside_root
            && pixels.is_some_and(|pixels| {
                pixels
                    .into_iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() <= 2)
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected displayed/managed geometry {expected:?}, managed={actual:?}, TranslateCoordinates={origin:?}, backing={backing:?}, root={root:?}, pixels={pixels:?}, ConfigureNotify={:?}",
            client.configures
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn motion(compositor: &HeadlessCompositor, x: i32, y: i32) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
}

pub(super) struct PressCoordinates {
    pub root: (i16, i16),
    pub screen: (i32, i32),
}

pub(super) fn press(compositor: &HeadlessCompositor, client: &mut Client) -> PressCoordinates {
    let [x, y, _, _] = geometry(compositor, client.id);
    let screen = (x + 60, y + 20);
    motion(compositor, screen.0, screen.1);
    client.clear_events();
    action(compositor, "pointer-button-down 272");
    let event = client.wait_press();
    assert_eq!(event.detail, 1);
    assert_eq!(
        (event.event_x, event.event_y),
        (60, 20),
        "fresh click must use current local coordinates"
    );
    let origin = client.origin();
    assert_eq!(
        (i32::from(event.root_x), i32::from(event.root_y)),
        (origin.0 + 60, origin.1 + 20)
    );
    PressCoordinates {
        root: (event.root_x, event.root_y),
        screen,
    }
}

pub(super) fn responsive_click(compositor: &HeadlessCompositor, client: &mut Client) {
    press(compositor, client);
    action(compositor, "pointer-button-up 272");
    client.wait_release();
    assert_eq!(client.presses.len(), 1);
    assert_eq!(
        client.releases.len(),
        1,
        "click must stay balanced after interaction cleanup"
    );
    client.paint_green();
    let expected = geometry(compositor, client.id);
    wait_geometry(compositor, client, expected, GREEN);
}

pub(super) fn wait_pixel(compositor: &HeadlessCompositor, x: u32, y: u32, color: [u8; 3]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        let index = (y * info.width + x) as usize * info.color_type.samples();
        if data[index..index + 3] == color {
            return;
        }
        assert!(Instant::now() < deadline, "X11 overlay was not rendered");
        thread::sleep(Duration::from_millis(20));
    }
}
