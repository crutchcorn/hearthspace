use super::super::session_lock::lock_client::LockClient;
use super::*;

#[test]
fn foreign_metadata_sanitizes_display_titles_and_keeps_mapping_identity() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let number = app.add("capture.metadata".into(), SIZE, (0, 0), RED);
    let raw = "Document\nMonitor FAKE\r\t\u{85}\u{2028}\u{2029}end";
    app.capture_title(number, raw);
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.metadata");
    assert_eq!(handle.title, "Document Monitor FAKE     end");
    assert_eq!(query(&compositor, "windows")["windows"][0]["title"], raw);
    assert!(!handle.identifier.is_empty());
    app.capture_title(number, &"é".repeat(600));
    app.capture_app_id(number, "capture.renamed");
    let renamed = client.toplevel("capture.renamed");
    assert_eq!(renamed.title.chars().count(), 512);
    assert_eq!(renamed.identifier, handle.identifier);
    let session = client.session(&renamed, false);
    assert_pixels(&client.capture(&session), |_, _| 0xffff0000);
    // A late shell role must retire the source, including existing sessions.
    app.capture_app_id(number, hearthspace_common::SHELL_OUTPUT_WATCHER_APP_ID);
    client.sync();
    assert!(client.state.sessions[&session.id].stopped);
    assert!(client.state.toplevels.iter().all(|t| t.closed));
}

#[test]
fn resizing_rejects_old_buffers_including_larger_buffers_after_shrink() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let number = app.add("capture.resize".into(), SIZE, (12, 8), RED);
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.resize");
    let session = client.session(&handle, false);
    let mut previous_size = SIZE;
    for size in [(400, 240), (180, 100)] {
        let obsolete = client.request(&session, previous_size);
        client.sync(); // Create the frame against the old constraint snapshot.
        app.repaint_with_size(number, size);
        client.sync();
        assert_eq!(client.state.sessions[&session.id].size, Some(size));
        obsolete.frame.capture();
        client.assert_failed(&obsolete);
        drop(obsolete);
        let current = client.capture(&session);
        assert_eq!(current.size, size);
        assert_pixels(&current, |_, _| 0xffff0000);
        previous_size = size;
    }
    assert_eq!(
        client.toplevel("capture.resize").identifier,
        handle.identifier
    );
}

#[test]
fn unmap_and_close_stop_sources_without_retargeting() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let number = app.add("capture.lifecycle".into(), SIZE, (0, 0), RED);
    let mut client = Client::from_connection(app.capture_connection());
    let old_handle = client.toplevel("capture.lifecycle");
    let old_session = client.session(&old_handle, false);
    let pending = client.request(&old_session, SIZE);
    client.sync();
    // One connection and one flush: the compositor receives capture before
    // the null-buffer commit, and both before offscreen frame processing.
    pending.frame.capture();
    app.unmap(number);
    client.assert_failed(&pending);
    assert!(client.state.sessions[&old_session.id].stopped);
    assert!(
        client
            .state
            .toplevels
            .iter()
            .find(|t| t.identifier == old_handle.identifier)
            .unwrap()
            .closed
    );
    app.remap(number, "capture.lifecycle");
    let new_handle = client.toplevel("capture.lifecycle");
    assert_ne!(new_handle.identifier, old_handle.identifier);
    let stale = client.session(&old_handle, false);
    assert!(client.state.sessions[&stale.id].stopped);
    let stale_frame = client.request(&stale, SIZE);
    stale_frame.frame.capture();
    client.assert_failed(&stale_frame);
    let current = client.session(&new_handle, false);
    assert_pixels(&client.capture(&current), |_, _| 0xffff0000);
    let closing = client.request(&current, SIZE);
    client.sync();
    closing.frame.capture();
    app.destroy(number);
    client.assert_failed(&closing);
    assert!(client.state.sessions[&current.id].stopped);
    assert!(client.state.toplevels.iter().all(|t| t.closed));
}

#[test]
fn frame_buffer_and_session_cancellation_never_write_pending_client_memory() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    app.add("capture.cancel".into(), SIZE, (0, 0), RED);
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.cancel");
    for kind in ["frame", "buffer", "session"] {
        let session = client.session(&handle, false);
        let cancelled = client.request(&session, SIZE);
        // Both requests travel in the same dispatch batch before offscreen work.
        cancelled.frame.capture();
        match kind {
            "frame" => cancelled.frame.destroy(),
            "buffer" => cancelled.buffer.destroy(),
            "session" => session.object.destroy(),
            _ => unreachable!(),
        }
        client.sync();
        if kind == "buffer" {
            client.assert_failed(&cancelled);
        }
        cancelled.assert_untouched();
        drop(cancelled);
        if kind != "session" {
            assert_pixels(&client.capture(&session), |_, _| 0xffff0000);
            session.object.destroy();
            client.sync();
        }
    }
    let survivor = client.session(&handle, false);
    assert_pixels(&client.capture(&survivor), |_, _| 0xffff0000);
}

#[test]
fn locking_stops_window_and_output_sessions_and_denies_new_admission() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    app.add("capture.lock".into(), SIZE, (0, 0), RED);
    let mut client = Client::from_connection(app.capture_connection());
    let handle = client.toplevel("capture.lock");
    let window = client.session(&handle, false);
    let output = client.output_session();
    let pending_window = client.request(&window, SIZE);
    let pending_output = client.request(&output, (800, 600));
    client.sync();
    let mut lock = LockClient::before_lock(app.capture_connection(), || {
        // The lock request follows these captures in the same protocol batch.
        pending_window.frame.capture();
        pending_output.frame.capture();
    });
    lock.wait_locked();
    client.assert_failed(&pending_window);
    client.assert_failed(&pending_output);
    assert!(client.state.sessions[&window.id].stopped);
    assert!(client.state.sessions[&output.id].stopped);
    let denied_window = client.session(&handle, false);
    let denied_output = client.output_session();
    assert!(client.state.sessions[&denied_window.id].stopped);
    assert!(client.state.sessions[&denied_output.id].stopped);
    for (session, size) in [(&denied_window, SIZE), (&denied_output, (800, 600))] {
        let frame = client.request(session, size);
        frame.frame.capture();
        client.assert_failed(&frame);
    }
    lock.unlock();
    let resumed = client.session(&handle, false);
    assert_pixels(&client.capture(&resumed), |_, _| 0xffff0000);
    assert!(
        client.state.sessions[&window.id].stopped,
        "unlock must not resume old grants"
    );
}
