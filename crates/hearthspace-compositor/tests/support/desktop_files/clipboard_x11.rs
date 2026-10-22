//! A real X11 owner offering only URI-list exercises file clipboard fallback.
use super::*;
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent,
            WindowClass,
        },
    },
    wrapper::ConnectionExt as _,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn desktop_file_clipboard_reads_uri_list_only_x11_owner() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    drop(compositor.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    };
    let (connection, screen_index) = x11rb::connect(Some(&display)).unwrap();
    let screen = &connection.setup().roots[screen_index];
    let owner = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            owner,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new(),
        )
        .unwrap();
    let atom = |name: &[u8]| {
        connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let clipboard = atom(b"CLIPBOARD");
    let targets = atom(b"TARGETS");
    let uri = atom(b"text/uri-list");
    connection
        .set_selection_owner(owner, clipboard, x11rb::CURRENT_TIME)
        .unwrap();
    connection.flush().unwrap();
    let stop = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        let service = scope.spawn(|| {
            let mut uri_requests = 0;
            let deadline = Instant::now() + Duration::from_secs(7);
            while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
                if let Some(Event::SelectionRequest(request)) = connection.poll_for_event().unwrap() {
                    let property = if request.target == targets {
                        connection.change_property32(PropMode::REPLACE, request.requestor,
                            request.property, AtomEnum::ATOM, &[targets, uri]).unwrap();
                        request.property
                    } else if request.target == uri {
                        uri_requests += 1;
                        connection.change_property8(PropMode::REPLACE, request.requestor,
                            request.property, uri, b"file:///tmp/X11%20song%231.ogg\r\n").unwrap();
                        request.property
                    } else { x11rb::NONE };
                    connection.send_event(false, request.requestor, EventMask::NO_EVENT,
                        SelectionNotifyEvent { response_type: smithay::reexports::x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
                            sequence: 0, time: request.time, requestor: request.requestor,
                            selection: request.selection, target: request.target, property }).unwrap();
                    connection.flush().unwrap();
                } else { thread::sleep(Duration::from_millis(2)); }
            }
            uri_requests
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let files = loop {
            let mut stream = UnixStream::connect(&compositor.command_socket_path).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream.write_all(b"desktop-files-clipboard-read\n").unwrap();
            let header = read_line(&mut stream);
            if let Some(size) = header.strip_prefix("ok ") {
                let mut bytes = vec![0; size.trim().parse().unwrap()];
                stream.read_exact(&mut bytes).unwrap();
                break serde_json::from_slice::<Value>(&bytes).unwrap();
            }
            if Instant::now() >= deadline {
                stop.store(true, Ordering::Release);
                panic!("X11 file clipboard failed: {header}");
            }
            thread::sleep(Duration::from_millis(10));
        };
        stop.store(true, Ordering::Release);
        assert_eq!(service.join().unwrap(), 1);
        files
    });
    assert_eq!(
        result,
        json!({"cut":false,"uris":["file:///tmp/X11%20song%231.ogg"]})
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
