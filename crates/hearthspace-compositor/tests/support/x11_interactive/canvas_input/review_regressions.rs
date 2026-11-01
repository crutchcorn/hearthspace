//! Real-client checks for producer confinement, keyboard replay, and XI2 state.
use super::*;
use smithay::reexports::x11rb::protocol::{
    xinput::{self as xi, ConnectionExt as _},
    xproto::{
        Allow, ButtonIndex, ChangeWindowAttributesAux, CreateWindowAux, EnterNotifyEvent,
        EventMask, GrabMode, GrabStatus, InputFocus, KeyButMask, KeyPressEvent, ModMask,
        MotionNotifyEvent, NotifyDetail, NotifyMode, WindowClass,
    },
};

#[path = "review_confinement.rs"]
mod confinement;
#[path = "review_enter_grab.rs"]
mod enter_grab;
#[path = "review_keyboard.rs"]
mod keyboard;
#[path = "review_raw_motion.rs"]
mod raw_motion;

#[derive(Debug, Default)]
struct Events {
    buttons: Vec<(bool, ButtonPressEvent)>,
    keys: Vec<(bool, KeyPressEvent)>,
    crossings: Vec<(bool, EnterNotifyEvent)>,
    xi_crossings: Vec<(bool, xi::EnterEvent)>,
    raw_motion: Vec<xi::RawMotionEvent>,
    motions: Vec<MotionNotifyEvent>,
}

impl Events {
    fn drain(&mut self, connection: &RustConnection) {
        while let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(e) => self.buttons.push((true, e)),
                Event::ButtonRelease(e) => self.buttons.push((false, e)),
                Event::KeyPress(e) => self.keys.push((true, e)),
                Event::KeyRelease(e) => self.keys.push((false, e)),
                Event::EnterNotify(e) => self.crossings.push((true, e)),
                Event::LeaveNotify(e) => self.crossings.push((false, e)),
                Event::XinputEnter(e) => self.xi_crossings.push((true, e)),
                Event::XinputLeave(e) => self.xi_crossings.push((false, e)),
                Event::XinputRawMotion(e) => self.raw_motion.push(e),
                Event::MotionNotify(e) => self.motions.push(e),
                Event::Error(e) => panic!("X11 review fixture protocol error: {e:?}"),
                _ => {}
            }
        }
    }

    fn assert_buttons(&self, root: Window, expected: &[(bool, Window, [i16; 2])]) {
        assert_eq!(self.buttons.len(), expected.len(), "{self:?}");
        for ((pressed, e), (want_press, recipient, point)) in self.buttons.iter().zip(expected) {
            assert_eq!((*pressed, e.event), (*want_press, *recipient), "{e:?}");
            assert_eq!([e.event_x, e.event_y], *point, "{e:?}");
            assert_eq!([e.root_x, e.root_y], *point, "{e:?}");
            assert_eq!((e.root, e.detail, e.response_type & 0x80), (root, 1, 0));
            assert!(e.same_screen);
            assert_eq!(e.state.contains(KeyButMask::BUTTON1), !*pressed);
        }
    }

    fn assert_keys(&self, root: Window, recipient: Window, count: usize) {
        assert_eq!(self.keys.len(), count, "{self:?}");
        for (index, (pressed, e)) in self.keys.iter().enumerate() {
            assert_eq!(*pressed, index % 2 == 0, "{e:?}");
            assert_eq!(e.event, recipient, "{e:?}");
            assert_eq!((e.root, e.detail, e.response_type & 0x80), (root, 38, 0));
            assert!(e.same_screen);
        }
    }
}

#[derive(Debug, Default)]
struct Observed {
    primary: Events,
    observer: Events,
}

impl Observed {
    fn drain(&mut self, w: &Windows) {
        self.primary.drain(&w.client.connection);
        self.observer.drain(&w.observer);
    }

    fn synchronize(&mut self, w: &Windows) {
        w.observer.get_input_focus().unwrap().reply().unwrap();
        w.client
            .connection
            .get_input_focus()
            .unwrap()
            .reply()
            .unwrap();
        self.drain(w);
    }

    fn wait(&mut self, w: &Windows, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.drain(w);
            if ready(self) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "missing DIX ingress fence: {self:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
        self.synchronize(w);
    }

    fn key_fence(&mut self, w: &Windows) {
        // The entire pointer prefix/key suffix is written before waiting.
        // Both devices enter MI on one Wayland connection; this unfrozen key
        // release fences frozen pointer ingress, not its eventual delivery.
        self.wait(w, |events| events.observer.keys.len() >= 2);
        self.observer.assert_keys(w.client.root, w.other, 2);
        assert!(self.primary.keys.is_empty(), "{self:?}");
    }

    fn pointer_fence(&mut self, w: &Windows) {
        // Inverse fence for frozen keyboard input: the key prefix precedes an
        // unfrozen pointer click. No hover or button is sent after restacking.
        self.wait(w, |events| events.primary.buttons.len() >= 2);
        self.primary.assert_buttons(
            w.client.root,
            &[
                (true, w.client.window, [60, 20]),
                (false, w.client.window, [60, 20]),
            ],
        );
        assert!(self.observer.buttons.is_empty(), "{self:?}");
    }
}

const KEY_MARKER: &str = "key-down 30\nkey-up 30\n";

fn keyboard_marker(w: &Windows) {
    let reply = w
        .observer
        .grab_keyboard(
            false,
            w.other,
            x11rb::CURRENT_TIME,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(reply.status, GrabStatus::SUCCESS);
}

fn buttons() -> EventMask {
    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE
}

fn select(connection: &RustConnection, window: Window, mask: EventMask) {
    let current = connection
        .get_window_attributes(window)
        .unwrap()
        .reply()
        .unwrap()
        .your_event_mask;
    connection
        .change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(current | mask),
        )
        .unwrap()
        .check()
        .unwrap();
}

fn frame(w: &Windows, window: Window) -> Window {
    let frame = w
        .observer
        .query_tree(window)
        .unwrap()
        .reply()
        .unwrap()
        .parent;
    assert_ne!(
        frame, w.client.root,
        "managed client must have an XWM frame"
    );
    frame
}

#[track_caller]
fn assert_pointer(w: &Windows, window: Window, child: Window, point: [i16; 2]) {
    let reply = w.observer.query_pointer(window).unwrap().reply().unwrap();
    assert!(reply.same_screen, "{reply:?}");
    assert_eq!(reply.root, w.client.root);
    assert_eq!(
        reply.child, child,
        "query window={window:#x}, root={:#x}, child={:#x}, root_xy=({},{}), win_xy=({},{}), mask={:?}",
        reply.root, reply.child, reply.root_x, reply.root_y, reply.win_x, reply.win_y, reply.mask
    );
    assert_eq!([reply.root_x, reply.root_y], point, "{reply:?}");
}

fn child(w: &Windows, at: [i16; 2], size: [u16; 2]) -> Window {
    let c = &w.client.connection;
    let screen = &c.setup().roots[0];
    let child = c.generate_id().unwrap();
    c.create_window(
        screen.root_depth,
        child,
        w.client.window,
        at[0],
        at[1],
        size[0],
        size[1],
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new().background_pixel(0x20c060),
    )
    .unwrap()
    .check()
    .unwrap();
    c.map_window(child).unwrap().check().unwrap();
    assert_eq!(
        c.query_tree(child).unwrap().reply().unwrap().parent,
        w.client.window
    );
    child
}

fn xi_pointer(w: &Windows) -> u16 {
    // QueryPointer establishes this client's core pointer selection first.
    w.observer.query_pointer(w.other).unwrap().reply().unwrap();
    let version = w
        .observer
        .xinput_xi_query_version(2, 2)
        .unwrap()
        .reply()
        .unwrap();
    assert!((version.major_version, version.minor_version) >= (2, 2));
    let pointer = w
        .observer
        .xinput_xi_get_client_pointer(w.other)
        .unwrap()
        .reply()
        .unwrap();
    assert!(
        pointer.set,
        "fixture must already have a core client pointer"
    );
    let devices = w
        .observer
        .xinput_xi_query_device(pointer.deviceid)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(devices.infos.len(), 1);
    assert_eq!(devices.infos[0].type_, xi::DeviceType::MASTER_POINTER);
    pointer.deviceid
}
