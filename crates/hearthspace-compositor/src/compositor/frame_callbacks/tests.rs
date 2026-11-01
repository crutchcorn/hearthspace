use smithay::utils::{Logical, Rectangle};

use super::{callback_output, test_support::Fixture};

#[test]
fn staggered_output_batches_preserve_each_clients_next_callback() {
    let mut f = Fixture::new();
    let a = f.window("A");
    let b = f.window("B");
    f.request_frame(a);
    f.request_frame(b);

    // Native completion tests can drive this same dispatcher with their real
    // pending-output gate. Here each batch stands for an accepted flip.
    for time in 1..=4 {
        assert_eq!(f.send_output("A", time * 2), 1);
        f.assert_done(&[(a, time * 2)]);
        f.request_frame(a);
        assert_eq!(f.send_output("B", time * 2 + 1), 1);
        f.assert_done(&[(b, time * 2 + 1)]);
        f.request_frame(b);
    }
    assert_eq!(f.send_output("A", 20), 1);
    assert_eq!(f.send_output("A", 21), 0);
    assert_eq!(f.send_output("B", 22), 1);
    f.assert_done(&[(a, 20), (b, 22)]);
}

#[test]
fn popup_and_subsurface_callbacks_follow_the_window_output() {
    let mut f = Fixture::new();
    let root = f.window("B");
    let child = f.subsurface(root);
    let popup = f.popup(root);
    let popup_child = f.subsurface(popup);
    for surface in [root, child, popup, popup_child] {
        f.request_frame(surface);
    }
    assert_eq!(f.send_output("A", 10), 0);
    f.assert_done(&[]);
    assert_eq!(f.send_output("B", 11), 4);
    f.assert_done(&[(root, 11), (child, 11), (popup, 11), (popup_child, 11)]);

    // Menus must keep repainting without a parent callback request.
    f.request_frame(popup);
    assert_eq!(f.send_output("B", 12), 1);
    f.assert_done(&[(popup, 12)]);
}

#[test]
fn hidden_and_offscreen_windows_keep_one_active_fallback_cadence() {
    let mut f = Fixture::new();
    let offscreen = f.window_at(rect(300, 50), true, None);
    let hidden = f.window_at(rect(110, 50), false, None);
    let shell = f.window_at(rect(0, 50), false, Some("B"));
    let unplugged = f.window_at(rect(300, 50), false, Some("retired"));
    for surface in [offscreen, hidden, shell, unplugged] {
        f.request_frame(surface);
    }
    assert_eq!(f.send_output("B", 1), 1);
    f.assert_done(&[(shell, 1)]);
    assert_eq!(f.send_output("A", 2), 3);
    f.assert_done(&[(offscreen, 2), (hidden, 2), (unplugged, 2)]);
}

#[test]
fn nested_and_headless_batches_still_service_all_windows() {
    let mut f = Fixture::new();
    let a = f.window("A");
    let b = f.window("B");
    f.request_frame(a);
    f.request_frame(b);
    assert_eq!(f.send(None, 5), 2);
    f.assert_done(&[(a, 5), (b, 5)]);
}

#[test]
fn locked_batches_only_service_ready_surfaces_on_the_completed_output() {
    let mut f = Fixture::new();
    let ordinary = f.window("A");
    let popup = f.popup(ordinary);
    f.request_frame(ordinary);
    f.request_frame(popup);
    f.lock();
    assert_eq!(
        f.send_output("A", 1),
        0,
        "an empty lock still isolates clients"
    );
    let a = f.lock_surface("A", true);
    let b = f.lock_surface("B", false);
    f.request_frame(a);
    f.request_frame(b);
    assert_eq!(f.send_output("B", 2), 0, "unready lock surface must wait");
    assert_eq!(f.send_output("A", 3), 1);
    f.assert_done(&[(a, 3)]);
    f.make_ready(b);
    assert_eq!(f.send_output("A", 4), 0);
    assert_eq!(f.send_output("B", 5), 1);
    f.assert_done(&[(b, 5)]);
    f.request_frame(a);
    f.destroy_lock_role(a);
    assert_eq!(
        f.send_output("A", 6),
        0,
        "destroyed role is no longer ready"
    );
    f.assert_done(&[]);
}

fn rect(x: i32, width: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, 0).into(), (width, 100).into())
}

#[test]
fn owner_uses_projected_overlap_policy_and_stable_ties() {
    let outputs = [("A", rect(0, 100)), ("B", rect(100, 100))];
    assert_eq!(
        callback_output(&outputs, Some(rect(80, 100)), |_| true, None),
        Some("B")
    );
    assert_eq!(
        callback_output(&outputs, Some(rect(50, 100)), |_| true, None),
        Some("A")
    );
    assert_eq!(
        callback_output(&outputs, Some(rect(80, 100)), |r| r == outputs[0].1, None),
        Some("A")
    );
    assert_eq!(
        callback_output(&outputs[1..], Some(rect(300, 10)), |_| false, Some("A")),
        Some("B")
    );
    assert_eq!(
        callback_output(&[], Some(rect(0, 100)), |_| true, None),
        None
    );
}
