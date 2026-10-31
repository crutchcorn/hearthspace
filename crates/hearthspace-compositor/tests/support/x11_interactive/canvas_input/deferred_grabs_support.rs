//! Raw event capture and an unfrozen-keyboard marker for deferred pointer input.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{
    CreateWindowAux, EventMask, KeyButMask, KeyPressEvent, MapState, WindowClass,
};

pub(super) const AB_WITH_KEY_MARKER: &str = concat!(
    "pointer-motion-abs 260 220\n",
    "pointer-button-down 272\n",
    "pointer-button-up 272\n",
    "pointer-motion-abs 680 340\n",
    "pointer-button-down 272\n",
    "pointer-button-up 272\n",
    "key-down 30\n",
    "key-up 30\n",
);

pub(super) const A_WITH_KEY_MARKER: &str = concat!(
    "pointer-motion-abs 260 220\n",
    "pointer-button-down 272\n",
    "pointer-button-up 272\n",
    "key-down 30\n",
    "key-up 30\n",
);

type ExpectedButton = (bool, Window, [i16; 2], [i16; 2]);

#[derive(Debug, Default)]
pub(super) struct RawEvents {
    pub buttons: Vec<(bool, ButtonPressEvent)>,
    keys: Vec<(bool, KeyPressEvent)>,
}

impl RawEvents {
    fn drain(&mut self, connection: &RustConnection) {
        while let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(event) => self.buttons.push((true, event)),
                Event::ButtonRelease(event) => self.buttons.push((false, event)),
                Event::KeyPress(event) => self.keys.push((true, event)),
                Event::KeyRelease(event) => self.keys.push((false, event)),
                Event::Error(error) => panic!("deferred input fixture error: {error:?}"),
                _ => {}
            }
        }
    }

    fn assert_buttons(&self, root: Window, expected: &[ExpectedButton]) {
        assert_eq!(self.buttons.len(), expected.len(), "{self:?}");
        for ((pressed, event), (want_press, recipient, local, root_point)) in
            self.buttons.iter().zip(expected)
        {
            assert_eq!(pressed, want_press, "{event:?}");
            assert_eq!(event.event, *recipient, "wrong recipient: {event:?}");
            assert_eq!([event.event_x, event.event_y], *local, "{event:?}");
            assert_eq!([event.root_x, event.root_y], *root_point, "{event:?}");
            assert_eq!(event.root, root, "{event:?}");
            assert_eq!(event.detail, 1, "{event:?}");
            assert!(event.same_screen, "{event:?}");
            assert_eq!(event.response_type & 0x80, 0, "synthetic input: {event:?}");
            assert_eq!(
                u16::from(event.state) & u16::from(KeyButMask::BUTTON1) != 0,
                !*pressed,
                "button state before the event: {event:?}"
            );
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Observed {
    pub primary: RawEvents,
    pub observer: RawEvents,
}

impl Observed {
    fn drain(&mut self, windows: &Windows) {
        self.primary.drain(&windows.client.connection);
        self.observer.drain(&windows.observer);
    }

    pub fn synchronize(&mut self, windows: &Windows) {
        // Requests after AllowEvents observe its synchronous DIX queue drain.
        // These X roundtrips alone do NOT fence incoming Wayland pointer data.
        windows.observer.get_input_focus().unwrap().reply().unwrap();
        windows
            .client
            .connection
            .get_input_focus()
            .unwrap()
            .reply()
            .unwrap();
        self.drain(windows);
    }

    pub fn wait_for_ingress(&mut self, windows: &Windows, markers: usize) {
        // batch() has already written the complete pointer prefix and key
        // suffix. This DIX-ingress inference relies on both devices arriving
        // on the same Xwayland Wayland connection and entering MI in order.
        // Receiving the unfrozen keyboard's release fences prior DIX ingress,
        // including EnqueueEvent for frozen pointer devices. It does NOT
        // acknowledge ordinary pointer delivery; assert that separately.
        // Keep these waits strictly after the one-write A/B input sequence.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.drain(windows);
            if self.observer.keys.len() >= markers * 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "missing DIX-ingress keyboard marker {markers}: {self:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
        self.synchronize(windows);
        assert!(self.primary.keys.is_empty(), "{self:?}");
        assert_eq!(self.observer.keys.len(), markers * 2, "{self:?}");
        for (index, (pressed, event)) in self.observer.keys.iter().enumerate() {
            assert_eq!(*pressed, index % 2 == 0, "{event:?}");
            assert_eq!(event.event, windows.other, "{event:?}");
            // Xwayland adds 8 to the evdev keycode; 30 is an unmodified A key.
            assert_eq!(event.detail, 38, "{event:?}");
            assert_eq!(event.response_type & 0x80, 0, "{event:?}");
        }
    }

    pub fn assert_buttons(
        &self,
        windows: &Windows,
        primary: &[ExpectedButton],
        observer: &[ExpectedButton],
    ) {
        self.primary.assert_buttons(windows.client.root, primary);
        self.observer.assert_buttons(windows.client.root, observer);
    }
}

pub(super) fn button_mask() -> EventMask {
    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE
}

pub(super) fn grab_keyboard(windows: &Windows, pointer_mode: GrabMode) {
    let grab = windows
        .observer
        .grab_keyboard(
            false,
            windows.other,
            x11rb::CURRENT_TIME,
            pointer_mode,
            GrabMode::ASYNC,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(grab.status, GrabStatus::SUCCESS);
}

pub(super) fn release_grabs(windows: &Windows) {
    windows
        .observer
        .ungrab_pointer(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    windows
        .observer
        .ungrab_keyboard(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
}

pub(super) fn mapped_child(windows: &Windows) -> Window {
    let connection = &windows.client.connection;
    let screen = &connection.setup().roots[0];
    let child = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            child,
            windows.client.window,
            40,
            0,
            120,
            80,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x20c060)
                .event_mask(button_mask()),
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(child).unwrap().check().unwrap();
    wait_pixel(&windows.compositor, 260, 220, GREEN);
    assert_eq!(
        connection
            .query_tree(child)
            .unwrap()
            .reply()
            .unwrap()
            .parent,
        windows.client.window
    );
    let origin = connection
        .translate_coordinates(child, windows.client.root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!((origin.dst_x, origin.dst_y), (40, 0));
    child
}

pub(super) fn map_popup(windows: &Windows, popup: Window) {
    let connection = &windows.client.connection;
    let screen = &connection.setup().roots[0];
    connection
        .create_window(
            screen.root_depth,
            popup,
            windows.client.root,
            0,
            0,
            120,
            80,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x20c060)
                .override_redirect(1)
                .event_mask(button_mask()),
        )
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[windows.client.window],
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(popup).unwrap().check().unwrap();
    wait_pixel(&windows.compositor, 260, 220, GREEN);
    assert_eq!(
        connection
            .query_tree(popup)
            .unwrap()
            .reply()
            .unwrap()
            .parent,
        windows.client.root,
        "the reused XID must be the surface's root child, not inside a WM frame"
    );
    let attributes = connection
        .get_window_attributes(popup)
        .unwrap()
        .reply()
        .unwrap();
    assert!(attributes.override_redirect);
    assert_eq!(attributes.map_state, MapState::VIEWABLE);
    let origin = connection
        .translate_coordinates(popup, windows.client.root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!((origin.dst_x, origin.dst_y), (0, 0));
}

pub(super) fn assert_scene(windows: &Windows) {
    assert_eq!(
        geometry(&windows.compositor, windows.client.id),
        [200, 200, 320, 240]
    );
    assert_eq!(
        geometry(&windows.compositor, windows.other_id),
        [600, 300, 320, 240]
    );
    assert_eq!(
        query(&windows.compositor, "status")["viewport"],
        windows.viewport
    );
}
