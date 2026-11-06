//! Touch emulation retains the originating surface even for overlapping roots.
use super::*;

#[test]
fn satellite_touch_clicks_keep_surface_recipients_and_exact_local_coordinates() {
    let _guard = headless_test_guard();
    let (compositor, display) = fixture();
    let a = WindowClient::new(&compositor, &display, "Touch A", 0xdc143c);
    let b = WindowClient::new(&compositor, &display, "Touch B", 0x204080);
    action(&compositor, format!("move-window {} 200 200", a.id));
    action(&compositor, format!("move-window {} 600 300", b.id));
    action(&compositor, "pan-to 0 0");
    assert_eq!(a.origin(), b.origin());
    a.drain(&mut Vec::new());
    b.drain(&mut Vec::new());
    batch(
        &compositor,
        concat!(
            "touch-down 71 0 260 220\n",
            "touch-frame 71\n",
            "touch-up 71 0\n",
            "touch-frame 71\n",
            "touch-down 71 1 680 340\n",
            "touch-frame 71\n",
            "touch-up 71 1\n",
            "touch-frame 71\n",
        ),
    );
    let a_events = wait_click(&a);
    let b_events = wait_click(&b);
    balanced(&a, &a_events, (60, 20));
    balanced(&b, &b_events, (80, 40));
}
