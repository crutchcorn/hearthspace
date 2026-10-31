//! An open X11 menu keeps its parent-local anchor as the canvas moves.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::ConfigureWindowAux;

fn green_bounds(compositor: &HeadlessCompositor) -> Option<[i32; 4]> {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let mut bounds = None;
    for (index, pixel) in data[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .enumerate()
    {
        if pixel[..3] != GREEN {
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

fn check_popup(scene: &mut Scene, popup: Window, position: [i32; 2], offset: [i16; 2]) {
    scene.move_input(false, [1100.0, 650.0]);
    scene.client.barrier(&scene.compositor);
    let [x, y] = scene.screen(position, offset);
    let expected = [
        x.round() as i32,
        y.round() as i32,
        (120.0 * SCALE).round() as i32,
        (80.0 * SCALE).round() as i32,
    ];
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = green_bounds(&scene.compositor);
        if actual.is_some_and(|bounds| {
            bounds
                .into_iter()
                .zip(expected)
                .zip([1, 1, 2, 2])
                .all(|((actual, expected), tolerance)| (actual - expected).abs() <= tolerance)
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "open popup must retain parent-local offset {offset:?} at canvas {position:?}: expected {expected:?}, pixels={actual:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    scene.client.clear_events();
    scene.move_input(
        false,
        scene.screen(position, [offset[0] + 20, offset[1] + 20]),
    );
    action(&scene.compositor, "pointer-button-down 272");
    action(&scene.compositor, "pointer-button-up 272");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut presses = 0;
    loop {
        match scene.observer.poll_for_event().unwrap() {
            Some(Event::ButtonPress(event)) => {
                assert_eq!(event.event, popup, "menu press reached the neighbor");
                assert_eq!(event.detail, 1);
                assert_local([event.event_x, event.event_y], [20, 20]);
                presses += 1;
            }
            Some(Event::ButtonRelease(event)) => {
                assert_eq!(event.event, popup, "menu release reached the neighbor");
                assert_eq!(event.detail, 1);
                assert_local([event.event_x, event.event_y], [20, 20]);
                assert_eq!(presses, 1);
                break;
            }
            Some(Event::Error(error)) => panic!("X11 popup error: {error:?}"),
            _ => thread::sleep(Duration::from_millis(10)),
        }
        assert!(
            Instant::now() < deadline,
            "open popup did not receive its click at {position:?}"
        );
    }
    scene.move_input(false, [1100.0, 650.0]);
    scene.client.barrier(&scene.compositor);
    assert!(
        scene.client.presses.is_empty() && scene.client.releases.is_empty(),
        "menu click leaked to its parent"
    );
    assert_eq!(
        query(&scene.compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    scene.placed(position);
    scene.unchanged();
}

fn move_and_pan(scene: &mut Scene, position: [i32; 2]) {
    action(
        &scene.compositor,
        format!(
            "move-window {} {} {}",
            scene.client.id, position[0], position[1]
        ),
    );
    action(
        &scene.compositor,
        format!("pan-to {} {}", position[0] - 471, position[1] - 291),
    );
    scene.viewport = query(&scene.compositor, "status")["viewport"].clone();
    assert_eq!(scene.viewport["x"], position[0] - 471);
    assert_eq!(scene.viewport["y"], position[1] - 291);
    assert_eq!(scene.viewport["scale"], SCALE);
    scene.client.barrier(&scene.compositor);
    scene.placed(position);
    scene.unchanged();
}

#[test]
fn x11_open_popup_follows_parent_across_canvas_boundaries_and_client_reposition() {
    let _guard = headless_test_guard();
    let initial = [100, 150];
    let mut scene = Scene::new(initial);
    let origin = scene.client.origin();
    let popup = window(
        &scene.observer,
        [origin.0 + 40, origin.1 + 60],
        [120, 80],
        true,
    );
    scene
        .observer
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[scene.client.window],
        )
        .unwrap();
    scene.observer.map_window(popup).unwrap();
    scene.observer.flush().unwrap();
    check_popup(&mut scene, popup, initial, [40, 60]);

    // Keep the same popup mapped, without sending it a configure. A changing
    // clamped parent root origin used to silently change this local offset.
    let negative = [89, -179];
    move_and_pan(&mut scene, negative);
    check_popup(&mut scene, popup, negative, [40, 60]);
    let far_positive = [90_000, 70_000];
    move_and_pan(&mut scene, far_positive);
    check_popup(&mut scene, popup, far_positive, [40, 60]);

    // Then honor an actual client reposition, relative to the current parent
    // TranslateCoordinates result, instead of freezing the first menu offset.
    let origin = scene.client.origin();
    scene
        .observer
        .configure_window(
            popup,
            &ConfigureWindowAux::new().x(origin.0 + 96).y(origin.1 + 112),
        )
        .unwrap();
    scene.observer.flush().unwrap();
    check_popup(&mut scene, popup, far_positive, [96, 112]);
    scene.observer.destroy_window(popup).unwrap();
    scene.observer.flush().unwrap();
    scene.client.barrier(&scene.compositor);
    let [x, y] = scene.screen(far_positive, [116, 132]);
    wait_pixel(&scene.compositor, x.round() as u32, y.round() as u32, RED);
    scene.placed(far_positive);
    scene.unchanged();
}
