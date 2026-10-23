//! Prewarmed shell cards retain their buffers without becoming desktop windows.
use super::*;

const PINK: [u8; 4] = [220, 0, 170, 255];
fn contains_pink(compositor: &HeadlessCompositor) -> bool {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let channels = info.color_type.samples();
    bytes[..info.buffer_size()]
        .chunks(channels)
        // The fake client writes little-endian ARGB (BGRA bytes), while PNG
        // readback exposes channels in RGB order.
        .any(|pixel| pixel[..3] == [PINK[2], PINK[1], PINK[0]])
}

#[test]
fn canvas_search_persistent_cards_reuse_buffers_without_visibility_focus_or_app_list_leaks() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (normal, _) = normal(&mut client, &compositor, "dev.hearthspace.PersistentSearch");
    action(
        &compositor,
        format!(
            "canvas-search-register {}",
            json!({"offset":0,"entries":[{"id":"control:sound","title":"Sound","search_text":"sound volume"}]})
        ),
    );
    let card = client.add(
        hearthspace_common::canvas_search_result_app_id(0, "control:sound"),
        (240, 72),
        (0, 0),
        PINK,
    );
    let unregistered = client.add(
        hearthspace_common::canvas_search_result_app_id(0, "control:unregistered"),
        (240, 72),
        (0, 0),
        PINK,
    );
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !contains_pink(&compositor),
        "prewarmed cards must be invisible before search"
    );
    let mut card_window = None;
    for _ in 0..2 {
        action(&compositor, "canvas-search-open");
        let opened = search(&compositor);
        let session = opened["session"].as_u64().unwrap();
        let field = field(&mut client, &opened);
        let shown = settled(&compositor);
        assert_eq!(result_ids(&shown), ["control:sound"]);
        let window = pose(&shown, "control:sound")["window_id"].as_u64().unwrap();
        if let Some(previous) = card_window {
            assert_eq!(window, previous, "same mapped surface serves both sessions");
        }
        card_window = Some(window);
        assert!(
            contains_pink(&compositor),
            "matched persistent card must be rendered"
        );
        assert_eq!(client.focus(), Some(field));
        update(&compositor, session, "no match");
        settled(&compositor);
        assert!(
            !contains_pink(&compositor),
            "nonmatching or unregistered card cannot leak through"
        );
        update(&compositor, session, "sound");
        settled(&compositor);
        assert!(contains_pink(&compositor));
        action(&compositor, format!("canvas-search-close {session}"));
        wait_search(&compositor, |s| s["scene"].as_array().unwrap().is_empty());
        assert!(
            !client.closed().contains(&card),
            "persistent card must remain warm"
        );
        assert!(!client.closed().contains(&unregistered));
        assert!(client.closed().contains(&field));
        assert_eq!(client.focus(), Some(normal));
        assert!(
            !contains_pink(&compositor),
            "closed search must hide warm surfaces"
        );
    }
}
