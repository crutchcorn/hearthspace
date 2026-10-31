//! Observe XWM dispatch without changing properties, focus, or stacking.
use super::*;

pub(super) fn xwm_barrier(observer: &RustConnection, requestor: Window) {
    // Smithay owns WM_S0, but rejects conversions for that non-data selection
    // with a synthetic SelectionNotify. Unlike the fixture's WM_NAME barrier,
    // this goes through no App property callback and sends no ConfigureWindow.
    // Its reply follows prior events and requests on the XWM connection, also
    // proving a preceding hover raise reached X before a foreign stack change.
    let selection = observer
        .intern_atom(true, b"WM_S0")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    assert_ne!(selection, x11rb::NONE);
    let owner = observer
        .get_selection_owner(selection)
        .unwrap()
        .reply()
        .unwrap()
        .owner;
    assert_ne!(owner, x11rb::NONE, "XWM must own WM_S0");
    observer
        .convert_selection(
            requestor,
            selection,
            AtomEnum::STRING.into(),
            AtomEnum::NONE,
            x11rb::CURRENT_TIME,
        )
        .unwrap();
    observer.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match observer.poll_for_event().unwrap() {
            Some(Event::SelectionNotify(event))
                if event.requestor == requestor && event.selection == selection =>
            {
                assert_ne!(
                    event.response_type & 0x80,
                    0,
                    "reply must originate from XWM"
                );
                assert_eq!(event.target, u32::from(AtomEnum::STRING));
                assert_eq!(event.property, x11rb::NONE);
                return;
            }
            Some(Event::ButtonPress(event) | Event::ButtonRelease(event)) => {
                panic!("unexpected input while waiting for XWM: {event:?}")
            }
            Some(Event::Error(error)) => panic!("XWM barrier protocol error: {error:?}"),
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(5)),
        }
        assert!(
            Instant::now() < deadline,
            "XWM did not answer observer barrier"
        );
    }
}
