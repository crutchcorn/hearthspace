//! Real search sessions copy through the ordinary Wayland clipboard protocol.
use super::*;
use hearthspace_common::canvas_search::calculator::RESULT_ID;

fn calculator(client: &mut MapClient) {
    client.add(
        hearthspace_common::canvas_search_result_app_id(0, RESULT_ID),
        (240, 72),
        (0, 0),
        GREEN,
    );
}

fn open(compositor: &HeadlessCompositor, client: &mut MapClient, expression: &str) -> u64 {
    action(compositor, "canvas-search-open");
    let opened = search(compositor);
    field(client, &opened);
    let session = opened["session"].as_u64().unwrap();
    update(compositor, session, expression);
    session
}

fn clipboard(client: &mut MapClient) -> String {
    String::from_utf8(client.clipboard_files("text/plain;charset=utf-8")).unwrap()
}

#[test]
fn canvas_search_calculation_activation_copies_answer_and_restores_previous_focus() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (app, _) = normal(&mut client, &compositor, "dev.hearthspace.Calculator");
    client.enable_file_drops(false);
    calculator(&mut client);
    let before = canonical(&compositor);
    let session = open(&compositor, &mut client, "6 * 7");
    let shown = settled(&compositor);
    assert_eq!(result_ids(&shown), [RESULT_ID]);
    assert_eq!(shown["selected_id"], RESULT_ID);
    assert_eq!(shown["results"][0]["title"], "42");
    assert_eq!(shown["results"][0]["kind"], "calculation");
    action(
        &compositor,
        format!(
            "canvas-search-activate {}",
            json!({"session":session,"id":null,"query":"6 * 7"})
        ),
    );
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(client.focus(), Some(app));
    assert_eq!(clipboard(&mut client), "42");
    assert_eq!(canonical(&compositor), before);
}

#[test]
fn canvas_search_calculation_pointer_click_copies_only_the_displayed_decimal() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (app, _) = normal(&mut client, &compositor, "dev.hearthspace.CalculatorClick");
    client.enable_file_drops(false);
    calculator(&mut client);
    open(&compositor, &mut client, "0.1 + 0.2");
    let shown = settled(&compositor);
    assert_eq!(shown["results"][0]["title"], "0.3");
    let bounds = &pose(&shown, RESULT_ID)["display"];
    click(
        &compositor,
        bounds["x"].as_f64().unwrap() + 120.,
        bounds["y"].as_f64().unwrap() + 36.,
    );
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(client.focus(), Some(app));
    assert_eq!(clipboard(&mut client), "0.3");
    assert!(
        client.presses().is_empty(),
        "result click leaked to the application"
    );
}

#[test]
fn canvas_search_calculation_query_change_and_cancellation_keep_the_clipboard() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    normal(&mut client, &compositor, "dev.hearthspace.CalculatorCancel");
    client.enable_file_drops(false);
    calculator(&mut client);
    action(&compositor, "clipboard-set 756e6368616e676564");
    let session = open(&compositor, &mut client, "6 * 7");
    let shown = settled(&compositor);
    let bounds = &pose(&shown, RESULT_ID)["display"];
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            bounds["x"].as_f64().unwrap() + 120.,
            bounds["y"].as_f64().unwrap() + 36.
        ),
    );
    action(&compositor, "pointer-button-down 272");
    update(&compositor, session, "7 * 8");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        search(&compositor)["active"],
        true,
        "held click activated a different answer"
    );
    assert_eq!(search(&compositor)["results"][0]["title"], "56");
    let stale = format!(
        "canvas-search-activate {}",
        json!({"session":session,"id":RESULT_ID,"query":"6 * 7"})
    );
    assert!(compositor.send_text_command(&stale).starts_with("err"));
    update(&compositor, session, "2 +");
    assert!(
        !result_ids(&search(&compositor))
            .iter()
            .any(|id| id == RESULT_ID)
    );
    action(&compositor, format!("canvas-search-close {session}"));
    assert_eq!(clipboard(&mut client), "unchanged");
    assert!(
        compositor
            .send_text_command(&format!(
                "canvas-search-activate {}",
                json!({"session":session,"id":RESULT_ID})
            ))
            .starts_with("err")
    );
    assert_eq!(clipboard(&mut client), "unchanged");
}
