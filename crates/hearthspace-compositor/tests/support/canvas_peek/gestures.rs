//! Interrupt an automatic slide through the real desktop pointer gesture.
use super::*;

const ITEM: &str = "file:7065656b2d67657374757265";
const HOME: [i64; 2] = [320, 300];
const PEEK: [i64; 2] = [252, 300];
const PRESS: [i64; 2] = [290, 340];
const MOVED: [i64; 2] = [120, 600];

fn sliding_icon() -> (HeadlessCompositor, MapClient, u64, usize) {
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env_remove("HEARTHSPACE_REDUCED_MOTION");
        });
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let (_, cover) = normal(
        &compositor,
        &mut client,
        "peek-gesture-cover",
        (400, 300),
        (800, 240),
    );
    let icon = client.add(
        hearthspace_common::desktop_item_app_id(ITEM),
        (100, 108),
        (0, 0),
        COLOR,
    );
    place(
        &compositor,
        json!({"kind":"item","id":ITEM}),
        "background",
        HOME[0] as f64,
        HOME[1] as f64,
    );
    wait_item(&compositor, ITEM, HOME);
    (compositor, client, cover, icon)
}

/// Leave the icon held at a proven intermediate pose, never a completed slide.
fn grab_sliding_icon(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    cover: u64,
    icon: usize,
    returning: bool,
) -> [i64; 2] {
    // x=290 is outside the covering app (including its resize border), but
    // inside the icon as it emerges. Probe that strip with real button input:
    // desktop-status has canonical positions, not animated screen bounds.
    // A successful grab adopts the visible position, making the interruption
    // observable without a screenshot or a sleep guessed against 300 ms.
    for _ in 0..4 {
        action(compositor, "canvas-search-motion false");
        action(compositor, format!("move-window {cover} 800 240"));
        wait_item(compositor, ITEM, HOME);
        action(
            compositor,
            format!("pointer-motion-abs {} {}", PRESS[0], PRESS[1]),
        );
        if returning {
            action(compositor, format!("move-window {cover} 300 240"));
            wait_item(compositor, ITEM, PEEK);
        }
        action(compositor, "canvas-search-motion true");
        let destination = if returning { 800 } else { 300 };
        action(compositor, format!("move-window {cover} {destination} 240"));
        wait_item(compositor, ITEM, if returning { HOME } else { PEEK });
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            action(compositor, "pointer-button-down 272");
            let state = query(compositor, "desktop-status");
            let item = state["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == ITEM)
                .unwrap();
            let grabbed = [item["x"].as_i64().unwrap(), item["y"].as_i64().unwrap()];
            if item["selected"] == true && grabbed[0] > PEEK[0] && grabbed[0] < HOME[0] {
                assert_eq!(grabbed[1], HOME[1]);
                assert_eq!(client.presses().last().map(|(index, _)| *index), Some(icon));
                // Motion is already cancelled by the press. Make the later
                // restoration assertions independent of animation scheduling.
                action(compositor, "canvas-search-motion false");
                return grabbed;
            }
            action(compositor, "pointer-button-up 272");
            if item["selected"] == true || Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        // Retry only acquisition if scheduling missed the entire slide. Once
        // an intermediate pose is grabbed, every behavior assertion runs once.
    }
    panic!("never grabbed the icon between its original home and peek destination");
}

fn move_held_icon(compositor: &HeadlessCompositor, grabbed: [i64; 2]) {
    action(
        compositor,
        format!(
            "pointer-motion-abs {} {}",
            PRESS[0] + MOVED[0] - grabbed[0],
            PRESS[1] + MOVED[1] - grabbed[1],
        ),
    );
    wait_item(compositor, ITEM, MOVED);
}

#[test]
fn canvas_peek_icon_click_during_slide_preserves_original_home() {
    let _guard = headless_test_guard();
    let (compositor, mut client, cover, icon) = sliding_icon();
    grab_sliding_icon(&compositor, &mut client, cover, icon, false);
    action(&compositor, "pointer-button-up 272");

    action(&compositor, format!("move-window {cover} 800 240"));
    wait_item(&compositor, ITEM, HOME);
    shutdown(compositor);
}

#[test]
fn canvas_peek_icon_escape_during_slide_preserves_original_home() {
    let _guard = headless_test_guard();
    let (compositor, mut client, cover, icon) = sliding_icon();
    let grabbed = grab_sliding_icon(&compositor, &mut client, cover, icon, false);
    move_held_icon(&compositor, grabbed);
    keys(&compositor, &[1]); // Escape cancels an actual move, not just a press.
    action(&compositor, "pointer-button-up 272");

    action(&compositor, format!("move-window {cover} 800 240"));
    wait_item(&compositor, ITEM, HOME);
    shutdown(compositor);
}

#[test]
fn canvas_peek_icon_committed_drag_during_slide_establishes_new_home() {
    let _guard = headless_test_guard();
    let (compositor, mut client, cover, icon) = sliding_icon();
    let grabbed = grab_sliding_icon(&compositor, &mut client, cover, icon, false);
    move_held_icon(&compositor, grabbed);
    action(&compositor, "pointer-button-up 272");

    action(&compositor, format!("move-window {cover} 800 240"));
    wait_item(&compositor, ITEM, MOVED);
    // A later automatic slide must return to the deliberate placement, rather
    // than either the original home or the interrupted animation sample.
    action(&compositor, format!("move-window {cover} 100 540"));
    wait_item(&compositor, ITEM, [52, 600]);
    action(&compositor, format!("move-window {cover} 800 240"));
    wait_item(&compositor, ITEM, MOVED);
    shutdown(compositor);
}

#[test]
fn canvas_peek_icon_click_or_cancel_during_return_finishes_original_home() {
    let _guard = headless_test_guard();
    let (compositor, mut client, cover, icon) = sliding_icon();
    for cancel in [false, true] {
        let grabbed = grab_sliding_icon(&compositor, &mut client, cover, icon, true);
        if cancel {
            move_held_icon(&compositor, grabbed);
            keys(&compositor, &[1]);
        }
        action(&compositor, "pointer-button-up 272");
        // The blocker is already gone: no extra scene change should be needed.
        wait_item(&compositor, ITEM, HOME);
    }
    shutdown(compositor);
}
