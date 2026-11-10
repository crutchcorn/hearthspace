//! Desktop URI copies cross App → Wayland → Satellite → XDND → actual X client.
use super::*;
use crate::{
    canvas_map_client::MapClient, desktop_files::position, navigation::keys, shell_panels::connect,
};
use serde_json::json;
#[path = "dnd/proxy_tests.rs"]
mod proxy_tests;
#[path = "dnd/receiver.rs"]
mod receiver;
use receiver::Receiver;

struct Scene {
    compositor: HeadlessCompositor,
    receiver: Receiver,
    client: MapClient,
    ids: Vec<String>,
    expected: Vec<u8>,
    scale: f64,
}

impl Scene {
    fn new(uris: Vec<String>) -> Self {
        let (compositor, display) = fixture();
        let receiver = Receiver::new(&compositor, &display);
        action(
            &compositor,
            format!("move-window {} 500 450", receiver.client.id),
        );
        action(&compositor, "pan-to 0 0");
        let mut client = connect(&compositor);
        let ids: Vec<_> = (0..uris.len())
            .map(|i| format!("file:{}", 61 + i))
            .collect();
        for id in &ids {
            client.add(
                hearthspace_common::desktop_item_app_id(id),
                (100, 108),
                (0, 0),
                [0, 255, 0, 255],
            );
        }
        action(
            &compositor,
            format!("desktop-register {}", json!({"offset":0,"ids":ids})),
        );
        for (offset, (id, uri)) in ids.iter().zip(&uris).enumerate() {
            action(
                &compositor,
                format!(
                    "desktop-file-register {}",
                    json!({"offset":offset,"entries":[{"id":id,"uri":uri}]})
                ),
            );
        }
        let expected = uris
            .into_iter()
            .flat_map(|uri| format!("{uri}\r\n").into_bytes())
            .collect();
        Self {
            compositor,
            receiver,
            client,
            ids,
            expected,
            scale: 1.0,
        }
    }

    fn press_file(&self, index: usize) {
        let point = position(&self.compositor, &self.ids[index]);
        let viewport = query(&self.compositor, "status")["viewport"].clone();
        let x = (point[0].as_f64().unwrap() - viewport["x"].as_f64().unwrap() + 44.0) * self.scale;
        let y = (point[1].as_f64().unwrap() - viewport["y"].as_f64().unwrap() + 40.0) * self.scale;
        action(&self.compositor, format!("pointer-motion-abs {x} {y}"));
        action(&self.compositor, "pointer-button-down 272");
    }

    fn select_all(&self) {
        action(
            &self.compositor,
            format!("desktop-select {}", json!(self.ids[0])),
        );
        action(&self.compositor, "key-down 29");
        for index in 1..self.ids.len() {
            self.press_file(index);
            action(&self.compositor, "pointer-button-up 272");
        }
        action(&self.compositor, "key-up 29");
    }

    fn begin(&mut self, index: usize) {
        crate::canvas_zoom::wait_scale(&self.compositor, self.scale);
        let viewport = query(&self.compositor, "status")["viewport"].clone();
        // Overlapping receivers can trigger canvas-settlement motion during
        // setup. Viewport animation being finished does not retire that motion.
        // Require the displayed receiver to reach its canonical projection
        // before sampling the exact client-local point used by the wire check.
        let window = wait_windows(&self.compositor, |windows| {
            windows.iter().any(|window| {
                window["id"] == self.receiver.client.id
                    && [("x", "canvas_x"), ("y", "canvas_y")]
                        .into_iter()
                        .all(|(axis, canvas)| {
                            window[axis].as_f64().unwrap()
                                == ((window[canvas].as_f64().unwrap()
                                    - viewport[axis].as_f64().unwrap())
                                    * self.scale)
                                    .round()
                        })
            })
        })
        .into_iter()
        .find(|w| w["id"] == self.receiver.client.id)
        .unwrap();
        assert_eq!(window["titlebar_height"], 0);
        let count = self.receiver.positions;
        self.press_file(index);
        // Apply the transform once; rounded window metadata is not an input origin.
        let x = (window["canvas_x"].as_f64().unwrap() - viewport["x"].as_f64().unwrap() + 60.0)
            * self.scale;
        let y = (window["canvas_y"].as_f64().unwrap() - viewport["y"].as_f64().unwrap() + 40.0)
            * self.scale;
        action(&self.compositor, format!("pointer-motion-abs {x} {y}"));
        self.receiver.wait(|r| r.positions > count);
        self.receiver.barrier(&self.compositor);
    }

    fn release(&self) {
        action(&self.compositor, "pointer-button-up 272");
    }
    fn locations(&self) -> Vec<serde_json::Value> {
        self.ids
            .iter()
            .map(|id| position(&self.compositor, id))
            .collect()
    }
    fn finish(mut self) {
        self.receiver.wait_owner_released();
        self.receiver.barrier(&self.compositor);
        self.client.sync();
        action(&self.compositor, "quit");
        wait_for_exit(&mut self.compositor.child);
    }
}

fn files() -> Vec<String> {
    vec![
        "file:///tmp/music%20%231%25.ogg".into(),
        "file:///tmp/caf%C3%A9%FF.ogg".into(),
    ]
}

#[test]
fn satellite_desktop_multi_file_drop_delivers_exact_uris_without_app_clicks() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    scene.select_all();
    let before = scene.locations();
    scene.begin(0);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(scene.receiver.dropped[0], scene.expected);
    assert_eq!(scene.locations(), before);
    scene.receiver.wait_owner_released();
    scene.receiver.barrier(&scene.compositor);
    // A completed offer must not poison SCTK's next enter or retain the old
    // selection payload when a different desktop item is dragged.
    action(
        &scene.compositor,
        format!("desktop-select {}", json!(scene.ids[1])),
    );
    scene.begin(1);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 2);
    assert_eq!(
        scene.receiver.dropped[1],
        b"file:///tmp/caf%C3%A9%FF.ogg\r\n"
    );
    assert_ne!(scene.receiver.sources[0], scene.receiver.sources[1]);
    assert_eq!(scene.locations(), before);
    scene.finish();
}

#[test]
fn satellite_desktop_large_uri_drop_completes_real_x11_incr() {
    let _guard = headless_test_guard();
    // Twelve legal URI strings, with percent-encoded spaces in short path
    // segments, exceed the bridge's bounded 64-KiB property chunk.
    let path = (0..10)
        .map(|_| "%20".repeat(210))
        .collect::<Vec<_>>()
        .join("/");
    let uris = (0..12)
        .map(|i| format!("file:///tmp/{path}/{i}.ogg"))
        .collect();
    let mut scene = Scene::new(uris);
    assert!(scene.expected.len() > 64 * 1024);
    scene.select_all();
    let before = scene.locations();
    scene.begin(0);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(scene.receiver.incr_transfers, 1);
    assert_eq!(scene.receiver.dropped[0], scene.expected);
    assert_eq!(scene.locations(), before);
    scene.finish();
}

#[test]
fn satellite_desktop_cancel_reject_and_stale_generation_do_not_deliver_newer_files() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    let before = scene.locations();
    scene.begin(0);
    let old = scene.receiver.sources[0];
    keys(&scene.compositor, &[1]);
    scene.release();
    scene.receiver.wait(|r| r.leaves == 1);
    assert!(scene.receiver.dropped.is_empty());

    scene.receiver.accept = false;
    scene.begin(1);
    assert_ne!(scene.receiver.sources[1], old);
    scene.receiver.stale_status(old);
    scene.receiver.stale_conversion(old);
    scene.receiver.barrier(&scene.compositor);
    scene.receiver.wait(|r| r.refused == 1);
    scene.release();
    scene.receiver.wait(|r| r.leaves == 2);
    assert!(scene.receiver.dropped.is_empty());
    assert_eq!(scene.locations(), before);

    scene.receiver.accept = true;
    scene.begin(1);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(
        scene.receiver.dropped[0],
        b"file:///tmp/caf%C3%A9%FF.ogg\r\n"
    );
    assert_eq!(scene.locations(), before);
    scene.finish();
}

#[test]
fn satellite_desktop_lock_and_source_removal_cancel_x11_offers_before_recovery() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    let before = scene.locations();
    scene.begin(0);
    let socket = scene
        .compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = crate::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    scene.receiver.wait(|r| r.leaves == 1);
    locker.unlock();
    scene.release();
    assert!(scene.receiver.dropped.is_empty());
    assert_eq!(scene.locations(), before);

    scene.begin(0);
    scene.client.destroy(0);
    scene.receiver.wait(|r| r.leaves == 2);
    scene.release();
    assert!(scene.receiver.dropped.is_empty());

    scene.begin(1);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(
        scene.receiver.dropped[0],
        b"file:///tmp/caf%C3%A9%FF.ogg\r\n"
    );
    scene.finish();
}

#[test]
fn satellite_desktop_destroyed_receiver_cannot_click_or_drop_into_its_replacement() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    let display = query(&scene.compositor, "status")["xwayland_display"]
        .as_str()
        .unwrap()
        .to_owned();
    let replacement = Receiver::from_client(WindowClient::new(
        &scene.compositor,
        &display,
        "X11 replacement",
        0x204080,
    ));
    action(
        &scene.compositor,
        format!("move-window {} 500 450", replacement.client.id),
    );
    action(
        &scene.compositor,
        format!("focus-window {}", scene.receiver.client.id),
    );
    action(&scene.compositor, "pan-to 0 0");
    scene.begin(0);
    scene
        .receiver
        .client
        .connection
        .destroy_window(scene.receiver.client.window)
        .unwrap()
        .check()
        .unwrap();
    wait_windows(&scene.compositor, |windows| {
        !windows.iter().any(|w| w["id"] == scene.receiver.client.id)
    });
    // The already configured neighbor is revealed under the held pointer.
    // No privileged move/focus command may bypass the in-progress drag guard.
    scene.receiver = replacement;
    scene.release();
    scene.receiver.barrier(&scene.compositor);
    assert!(scene.receiver.dropped.is_empty());
    assert!(
        scene.receiver.sources.is_empty(),
        "replacement inherited a cancelled drag"
    );
    scene.begin(1);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(
        scene.receiver.dropped[0],
        b"file:///tmp/caf%C3%A9%FF.ogg\r\n"
    );
    scene.finish();
}

#[test]
fn satellite_desktop_drop_keeps_local_coordinates_at_zoomed_and_panned_canvas() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    scene.scale = 0.864;
    action(&scene.compositor, "zoom-to 0.864");
    crate::canvas_zoom::wait_scale(&scene.compositor, scene.scale);
    action(&scene.compositor, "pan-to 20 10");
    scene.select_all();
    let before = scene.locations();
    scene.begin(0);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(scene.receiver.dropped[0], scene.expected);
    assert_eq!(scene.locations(), before);
    let window = query(&scene.compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == scene.receiver.client.id)
        .unwrap()
        .clone();
    assert_eq!(
        (
            window["canvas_x"].as_i64().unwrap(),
            window["canvas_y"].as_i64().unwrap()
        ),
        (500, 450)
    );
    scene.finish();
}
