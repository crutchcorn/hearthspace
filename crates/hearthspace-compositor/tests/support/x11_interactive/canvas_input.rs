//! Canvas placement must not clip real X11 input at the X root's boundaries.
use super::*;
use serde_json::Value;
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ButtonPressEvent, ConnectionExt, CreateGCAux, PropMode, Rectangle, Window,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

const SCALE: f64 = 0.864;
const BLUE: [u8; 3] = [32, 64, 128];

#[path = "canvas_input/batched_input.rs"]
mod batched_input;
#[path = "canvas_input/group_move.rs"]
mod group_move;
#[path = "canvas_input/popup_motion.rs"]
mod popup_motion;
#[path = "canvas_input/window.rs"]
mod window_fixture;
use window_fixture::window;

fn assert_local(actual: [i16; 2], expected: [i16; 2]) {
    // Existing fractional-zoom X11 coverage permits one pixel of rounding.
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, e)| (i32::from(a) - i32::from(e)).abs() <= 1),
        "X11 client-local coordinates: expected {expected:?}, received {actual:?}"
    );
}

struct Scene {
    compositor: HeadlessCompositor,
    client: Client,
    observer: RustConnection,
    other_id: u64,
    other_position: [i32; 2],
    viewport: Value,
}

impl Scene {
    fn new(position: [i32; 2]) -> Self {
        let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
        drop(compositor.wait_for_socket());
        let mut client = Client::connect(&compositor);
        let status = query(&compositor, "status");
        let (observer, _) = x11rb::connect(status["xwayland_display"].as_str()).unwrap();
        let other = window(&observer, [650, 150], [320, 240], false);
        observer.map_window(other).unwrap();
        observer.flush().unwrap();
        let other_id = wait_windows(&compositor, |w| {
            w.len() == 2 && w.iter().all(|w| w["mapped"] == true)
        })
        .into_iter()
        .find(|w| w["id"] != client.id)
        .unwrap()["id"]
            .as_u64()
            .unwrap();
        action(&compositor, format!("focus-window {}", client.id));
        let other_position = [position[0] + 240, position[1]];
        action(
            &compositor,
            format!(
                "move-window {other_id} {} {}",
                other_position[0], other_position[1]
            ),
        );
        action(
            &compositor,
            format!("move-window {} {} {}", client.id, position[0], position[1]),
        );
        action(&compositor, format!("zoom-to {SCALE}"));
        crate::canvas_zoom::wait_scale(&compositor, SCALE);
        // Reproduce the visible (407, 251) origin, including the reported
        // canvas (89, -179) case, without prescribing any X root placement.
        action(
            &compositor,
            format!("pan-to {} {}", position[0] - 471, position[1] - 291),
        );
        client.barrier(&compositor);
        let viewport = query(&compositor, "status")["viewport"].clone();
        assert_eq!(viewport["x"], position[0] - 471);
        assert_eq!(viewport["y"], position[1] - 291);
        assert_eq!(viewport["scale"], SCALE);
        let scene = Self {
            compositor,
            client,
            observer,
            other_id,
            other_position,
            viewport,
        };
        scene.placed(position);
        let [x, y] = scene.screen(other_position, [280, 120]);
        wait_pixel(&scene.compositor, x.round() as u32, y.round() as u32, BLUE);
        scene.unchanged();
        scene
    }

    fn screen(&self, position: [i32; 2], local: [i16; 2]) -> [f64; 2] {
        [0, 1].map(|axis| {
            (f64::from(position[axis]) - self.viewport[["x", "y"][axis]].as_f64().unwrap()
                + f64::from(local[axis]))
                * SCALE
        })
    }

    fn placed(&self, position: [i32; 2]) {
        let windows = wait_windows(&self.compositor, |windows| {
            windows.iter().any(|w| {
                w["id"] == self.client.id
                    && w["canvas_x"] == position[0]
                    && w["canvas_y"] == position[1]
            })
        });
        let w = windows.iter().find(|w| w["id"] == self.client.id).unwrap();
        assert_eq!(
            geometry(&self.compositor, self.client.id),
            [position[0], position[1], 320, 240]
        );
        assert_eq!(w["titlebar_height"], 0);
        assert_eq!(w["visible"], true);
        for (field, expected) in ["x", "y"].into_iter().zip(self.screen(position, [0, 0])) {
            assert_eq!(
                w[field].as_i64().unwrap(),
                expected.round() as i64,
                "displayed {field}"
            );
        }
        let [x, y] = self.screen(position, [20, 100]);
        wait_pixel(&self.compositor, x.round() as u32, y.round() as u32, RED);
    }

    fn unchanged(&self) {
        assert_eq!(
            query(&self.compositor, "status")["viewport"],
            self.viewport,
            "client input changed the viewport"
        );
        assert_eq!(
            geometry(&self.compositor, self.other_id),
            [self.other_position[0], self.other_position[1], 320, 240]
        );
        self.observer.get_input_focus().unwrap().reply().unwrap();
        while let Some(event) = self.observer.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(e) | Event::ButtonRelease(e) => {
                    panic!("input leaked to another X11 window: {e:?}")
                }
                Event::Error(error) => panic!("X11 observer protocol error: {error:?}"),
                _ => {}
            }
        }
    }

    fn move_input(&self, touch: bool, point: [f64; 2]) {
        let [x, y] = point;
        if touch {
            action(&self.compositor, format!("touch-motion 71 0 {x} {y}"));
            action(&self.compositor, "touch-frame 71");
        } else {
            action(&self.compositor, format!("pointer-motion-abs {x} {y}"));
        }
    }

    fn popup(&mut self, position: [i32; 2]) {
        // Real toolkits position menus relative to TranslateCoordinates, which
        // is free to differ from the parent's canvas home.
        let origin = self.client.origin();
        let popup = window(
            &self.observer,
            [origin.0 + 40, origin.1 + 60],
            [120, 80],
            true,
        );
        self.observer
            .change_property32(
                PropMode::REPLACE,
                popup,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                &[self.client.window],
            )
            .unwrap();
        self.observer.map_window(popup).unwrap();
        self.observer.flush().unwrap();
        let [x, y] = self.screen(position, [60, 80]);
        wait_pixel(&self.compositor, x.round() as u32, y.round() as u32, GREEN);
        self.client.clear_events();
        self.move_input(false, [x, y]);
        action(&self.compositor, "pointer-button-down 272");
        action(&self.compositor, "pointer-button-up 272");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut presses = 0;
        loop {
            match self.observer.poll_for_event().unwrap() {
                Some(Event::ButtonPress(e)) => {
                    assert_eq!(e.event, popup, "popup press reached the other window");
                    assert_eq!(e.detail, 1);
                    assert_local([e.event_x, e.event_y], [20, 20]);
                    presses += 1;
                }
                Some(Event::ButtonRelease(e)) => {
                    assert_eq!(e.event, popup, "popup release reached the other window");
                    assert_local([e.event_x, e.event_y], [20, 20]);
                    assert_eq!(presses, 1);
                    break;
                }
                Some(Event::Error(error)) => panic!("X11 popup protocol error: {error:?}"),
                _ => thread::sleep(Duration::from_millis(10)),
            }
            assert!(
                Instant::now() < deadline,
                "anchored popup did not receive its click"
            );
        }
        self.observer.destroy_window(popup).unwrap();
        self.observer.flush().unwrap();
        self.move_input(false, [1100.0, 650.0]);
        self.client.barrier(&self.compositor);
        assert!(
            self.client.presses.is_empty(),
            "popup click reached its parent"
        );
        wait_pixel(&self.compositor, x.round() as u32, y.round() as u32, RED);
        self.placed(position);
        self.unchanged();
    }

    fn close(&mut self, position: [i32; 2]) {
        let connection = &self.client.connection;
        let target = self.client.window;
        let gc = connection.generate_id().unwrap();
        connection
            .create_gc(gc, target, &CreateGCAux::new().foreground(0x20c060))
            .unwrap();
        connection
            .poly_fill_rectangle(
                target,
                gc,
                &[Rectangle {
                    x: 280,
                    y: 0,
                    width: 40,
                    height: 34,
                }],
            )
            .unwrap();
        connection.free_gc(gc).unwrap();
        connection.flush().unwrap();
        let [x, y] = self.screen(position, [300, 17]);
        wait_pixel(&self.compositor, x.round() as u32, y.round() as u32, GREEN);
        self.move_input(false, [x, y]);
        self.client.clear_events();
        action(&self.compositor, "pointer-button-down 272");
        action(&self.compositor, "pointer-button-up 272");
        let press = self.client.wait_press();
        self.client.wait_release();
        assert_eq!(self.client.presses.len(), 1);
        assert_eq!(self.client.releases.len(), 1);
        let release = self.client.releases[0];
        for event in [press, release] {
            assert_eq!(event.detail, 1);
            assert_local([event.event_x, event.event_y], [300, 17]);
        }
        self.placed(position);
        // The fixture's close handler runs only for a real click in its CSD
        // control; an IPC close or unconditional teardown would miss this bug.
        let in_close =
            |e: ButtonPressEvent| (280..320).contains(&e.event_x) && (0..34).contains(&e.event_y);
        if in_close(press) && in_close(release) {
            self.client.connection.destroy_window(target).unwrap();
            self.client.connection.flush().unwrap();
        }
        wait_windows(&self.compositor, |w| {
            w.len() == 1 && w[0]["id"] == self.other_id
        });
        self.move_input(false, [1100.0, 650.0]);
        // The other window lies beneath this control and survives the click.
        wait_pixel(&self.compositor, x.round() as u32, y.round() as u32, BLUE);
        self.unchanged();
    }
}

fn drag_and_close(position: [i32; 2], touch: bool) {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(position);
    let [x, y] = scene.screen(position, [134, 17]);
    scene.client.clear_events();
    if touch {
        action(&scene.compositor, format!("touch-down 71 0 {x} {y}"));
        action(&scene.compositor, "touch-frame 71");
    } else {
        scene.move_input(false, [x, y]);
        action(&scene.compositor, "pointer-button-down 272");
    }
    // Include motion preceding a toolkit's delayed _NET_WM_MOVERESIZE request.
    scene.move_input(touch, [x + 16.0 * SCALE, y + 8.0 * SCALE]);
    let press = scene.client.wait_press();
    assert_eq!(press.detail, 1);
    assert_local([press.event_x, press.event_y], [134, 17]);
    scene.client.barrier(&scene.compositor);
    let pointer = scene
        .client
        .connection
        .query_pointer(scene.client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert!(pointer.same_screen);
    assert_local([pointer.win_x, pointer.win_y], [150, 25]);
    let origin = scene.client.origin();
    assert_eq!(
        [i32::from(press.root_x), i32::from(press.root_y)],
        [
            origin.0 + i32::from(press.event_x),
            origin.1 + i32::from(press.event_y)
        ]
    );
    scene.placed(position);
    scene.unchanged();
    scene
        .client
        .request(&scene.compositor, 8, 1, (press.root_x, press.root_y));
    scene.client.clear_events();
    scene.move_input(touch, [x + 100.0 * SCALE, y + 50.0 * SCALE]);
    let moved = [position[0] + 100, position[1] + 50];
    scene.placed(moved);
    scene.client.barrier(&scene.compositor);
    assert_eq!(
        scene.client.motions, 0,
        "CSD drag motion leaked into the client"
    );
    if touch {
        action(&scene.compositor, "touch-up 71 0");
        action(&scene.compositor, "touch-frame 71");
    } else {
        action(&scene.compositor, "pointer-button-up 272");
    }
    scene.move_input(false, [1100.0, 650.0]);
    scene.client.barrier(&scene.compositor);
    scene.placed(moved);
    scene.unchanged();
    scene.popup(moved);
    scene.close(moved);
}

#[test]
fn x11_negative_canvas_pointer_csd_move_popup_and_close() {
    drag_and_close([89, -179], false);
}

#[test]
fn x11_far_positive_canvas_pointer_csd_move_popup_and_close() {
    drag_and_close([90_000, 70_000], false);
}

#[test]
fn x11_negative_canvas_touch_csd_move_popup_and_close() {
    drag_and_close([89, -179], true);
}

#[test]
fn x11_far_positive_canvas_touch_csd_move_popup_and_close() {
    drag_and_close([90_000, 70_000], true);
}
