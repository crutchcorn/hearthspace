//! Normal clock dropdown changes share the persisted per-output layout setting.
use super::*;

fn set_format(output: &str, value: &str) -> String {
    format!(
        "frame-clock-format {}",
        json!({"output":output,"value":value})
    )
}

#[test]
fn clock_format_outside_layout_editing_updates_and_survives_restart() {
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let initial = layout(&compositor);
    let output = initial["outputs"][0]["output"].as_str().unwrap();
    let placement = initial["outputs"][0]["layout"]["placements"].clone();
    for format in ["weekday", "short", "numeric", "none", "long"] {
        action(&compositor, set_format(output, format));
        let current = layout(&compositor);
        assert_eq!(current["editing"], false);
        assert_eq!(current["session"], 0);
        assert_eq!(current["outputs"][0]["layout"]["clock_date_format"], format);
        assert_eq!(current["outputs"][0]["layout"]["placements"], placement);
    }
    let before = layout(&compositor);
    action(&compositor, set_format(output, "long"));
    assert_eq!(
        layout(&compositor)["revision"],
        before["revision"],
        "same format is a no-op"
    );
    for invalid in [
        set_format("disconnected-output", "none"),
        set_format(output, "invalid"),
        format!("frame-clock-format {}", json!({"value":"none"})),
        format!(
            "frame-clock-format {}",
            json!({"output":output,"value":"none","extra":true})
        ),
    ] {
        assert!(
            compositor.send_text_command(&invalid).starts_with("err "),
            "{invalid}"
        );
        assert_eq!(layout(&compositor)["outputs"], before["outputs"]);
    }
    action(&compositor, "layout-tool");
    assert!(
        compositor
            .send_text_command(&set_format(output, "none"))
            .starts_with("err ")
    );
    assert_eq!(
        layout(&compositor)["outputs"][0]["layout"]["clock_date_format"],
        "long"
    );
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);

    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let restored = layout(&compositor);
    assert_eq!(
        restored["outputs"][0]["layout"]["clock_date_format"],
        "long"
    );
    assert_eq!(restored["outputs"][0]["layout"]["placements"], placement);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
