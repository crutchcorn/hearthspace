use super::*;

#[test]
fn x11_csd_touch_drag_keeps_x_coordinates_and_recovers_after_touch_cancel() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    client.clear_events();
    action(&compositor, "touch-down 71 0 160 170");
    action(&compositor, "touch-frame 71");
    action(&compositor, "touch-motion 71 0 176 178");
    action(&compositor, "touch-frame 71");
    let press = client.wait_press();
    assert_eq!(
        press.detail, 1,
        "touch must reach XWayland as a primary press"
    );
    assert_eq!((press.event_x, press.event_y), (60, 20));
    client.request(&compositor, 8, 1, (press.root_x, press.root_y));
    action(&compositor, "touch-motion 71 0 220 200");
    action(&compositor, "touch-frame 71");
    wait_geometry(&compositor, &mut client, [160, 180, 320, 240], RED);
    action(&compositor, "touch-up 71 0");
    action(&compositor, "touch-frame 71");
    motion(&compositor, 600, 500);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [160, 180, 320, 240], RED);
    responsive_click(&compositor, &mut client);

    client.clear_events();
    action(&compositor, "touch-down 71 1 220 200");
    action(&compositor, "touch-frame 71");
    action(&compositor, "touch-motion 71 1 236 208");
    action(&compositor, "touch-frame 71");
    let press = client.wait_press();
    client.request(&compositor, 8, 1, (press.root_x, press.root_y));
    action(&compositor, "touch-motion 71 1 250 220");
    action(&compositor, "touch-frame 71");
    wait_geometry(&compositor, &mut client, [190, 200, 320, 240], GREEN);
    action(&compositor, "touch-cancel 71 1");
    action(&compositor, "touch-frame 71");
    motion(&compositor, 700, 550);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [190, 200, 320, 240], GREEN);
    responsive_click(&compositor, &mut client);
    let point = super::press(&compositor, &mut client);
    client.request(&compositor, 8, 1, point.root);
    motion(&compositor, point.screen.0 + 20, point.screen.1 + 10);
    action(&compositor, "pointer-button-up 272");
    wait_geometry(&compositor, &mut client, [210, 210, 320, 240], GREEN);
}
