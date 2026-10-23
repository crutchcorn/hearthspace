use super::super::interactive::client::gtk::ConfigureEvent;
use super::*;

fn sequence(edges: &[u32]) -> Vec<ConfigureEvent> {
    vec![
        ConfigureEvent::Gtk,
        ConfigureEvent::Edges(edges.to_vec()),
        ConfigureEvent::Toplevel,
        ConfigureEvent::Surface,
    ]
}

#[test]
fn gtk_titlebar_size_hint_commits_refresh_edges_once_before_xdg_configure() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    client.gtk_configure_events(0);
    let count = client.configuration(0).0;
    client.set_size_limits(0, (240, 0), (240, 0));
    assert_eq!(client.gtk_configure_events(0), sequence(&[1, 3]));
    assert_eq!(client.configuration(0).0, count + 1);
    assert_eq!(client.gtk_edges(0), [1, 3]);
    assert!(
        client.gtk_configure_events(0).is_empty(),
        "configure acknowledgment must settle"
    );

    // New limits with the same permitted edges do not require another configure.
    client.set_size_limits(0, (260, 0), (260, 0));
    assert!(client.gtk_configure_events(0).is_empty());
    client.set_size_limits(0, (0, 0), (0, 0));
    assert_eq!(client.gtk_configure_events(0), sequence(&[1, 2, 3, 4]));
    assert_eq!(client.configuration(0).0, count + 2);
    assert!(client.gtk_configure_events(0).is_empty());

    client.gtk_request_maximized(0, true);
    state(&compositor, &mut client, id, true, true);
    client.gtk_configure_events(0);
    let bounded_count = client.configuration(0).0;
    client.set_size_limits(0, (240, 0), (240, 0));
    assert!(client.gtk_configure_events(0).is_empty());
    client.set_size_limits(0, (0, 0), (0, 0));
    assert!(client.gtk_configure_events(0).is_empty());
    assert_eq!(client.configuration(0).0, bounded_count);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn gtk_titlebar_v1_never_receives_v2_edges_or_hint_only_configures() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = InteractiveClient::connect(&socket(&compositor));
    wait_windows(&compositor, |windows| windows.len() == 2);
    client.gtk_use_version(0, 1);
    assert_eq!(
        client.gtk_configure_events(0),
        [
            ConfigureEvent::Gtk,
            ConfigureEvent::Toplevel,
            ConfigureEvent::Surface
        ]
    );
    let count = client.configuration(0).0;
    client.set_size_limits(0, (240, 0), (240, 0));
    assert!(client.gtk_configure_events(0).is_empty());
    assert_eq!(client.configuration(0).0, count);
    client.request_restore(0);
    let events = client.gtk_configure_events(0);
    assert!(!events.is_empty());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ConfigureEvent::Edges(_)))
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
