//! A tiny CSD X11 app: requests originate from actual X button events.
use super::*;
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ButtonPressEvent, ChangeWindowAttributesAux, ClientMessageEvent,
            ConfigureNotifyEvent, ConnectionExt, CreateWindowAux, EventMask, PropMode,
            ResizeRequestEvent, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

pub(super) const RED: [u8; 3] = [220, 20, 60];
pub(super) const GREEN: [u8; 3] = [32, 192, 96];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Crossing {
    Enter,
    Leave,
}

pub(super) struct Client {
    pub connection: RustConnection,
    pub root: Window,
    pub window: Window,
    pub id: u64,
    moveresize: Atom,
    sequence: u64,
    pub presses: Vec<ButtonPressEvent>,
    pub releases: Vec<ButtonPressEvent>,
    pub configures: Vec<ConfigureNotifyEvent>,
    pub resize_requests: Vec<ResizeRequestEvent>,
    pub motions: usize,
    pub crossings: Vec<Crossing>,
}

impl Client {
    pub fn connect(compositor: &HeadlessCompositor) -> Self {
        Self::connect_with_identity(compositor, Some(b"x11-csd-fixture\0X11CsdFixture\0"), None)
    }

    pub fn connect_with_identity(
        compositor: &HeadlessCompositor,
        class: Option<&[u8]>,
        reported_pid: Option<u32>,
    ) -> Self {
        let deadline = Instant::now() + Duration::from_secs(10);
        let display = loop {
            if let Some(display) = query(compositor, "status")["xwayland_display"].as_str() {
                break display.to_owned();
            }
            assert!(Instant::now() < deadline, "XWayland did not become ready");
            thread::sleep(Duration::from_millis(20));
        };
        let (connection, index) = x11rb::connect(Some(&display)).unwrap();
        let screen = &connection.setup().roots[index];
        let root = screen.root;
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                screen.root_depth,
                window,
                root,
                100,
                150,
                320,
                240,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    .background_pixel(0xdc143c)
                    .event_mask(
                        EventMask::STRUCTURE_NOTIFY
                            | EventMask::BUTTON_PRESS
                            | EventMask::BUTTON_RELEASE
                            | EventMask::POINTER_MOTION
                            | EventMask::ENTER_WINDOW
                            | EventMask::LEAVE_WINDOW,
                    ),
            )
            .unwrap();
        let motif = connection
            .intern_atom(false, b"_MOTIF_WM_HINTS")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(PropMode::REPLACE, window, motif, motif, &[2, 0, 0, 0, 0])
            .unwrap();
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"X11 CSD fixture",
            )
            .unwrap();
        if let Some(class) = class {
            connection
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    AtomEnum::WM_CLASS,
                    AtomEnum::STRING,
                    class,
                )
                .unwrap();
        }
        if let Some(pid) = reported_pid {
            let atom = connection
                .intern_atom(false, b"_NET_WM_PID")
                .unwrap()
                .reply()
                .unwrap()
                .atom;
            connection
                .change_property32(PropMode::REPLACE, window, atom, AtomEnum::CARDINAL, &[pid])
                .unwrap();
        }
        let moveresize = connection
            .intern_atom(false, b"_NET_WM_MOVERESIZE")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection.map_window(window).unwrap();
        connection.flush().unwrap();
        let windows = wait_windows(compositor, |windows| {
            windows.len() == 1 && windows[0]["mapped"] == true
        });
        assert_eq!(
            windows[0]["titlebar_height"], 0,
            "Motif hints must select CSD"
        );
        let id = windows[0]["id"].as_u64().unwrap();
        action(compositor, format!("move-window {id} 100 150"));
        Self {
            connection,
            root,
            window,
            id,
            moveresize,
            sequence: 0,
            presses: Vec::new(),
            releases: Vec::new(),
            configures: Vec::new(),
            resize_requests: Vec::new(),
            motions: 0,
            crossings: Vec::new(),
        }
    }

    pub fn pump(&mut self) {
        while let Some(event) = self.connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(event) if event.event == self.window => self.presses.push(event),
                Event::ButtonRelease(event) if event.event == self.window => {
                    self.releases.push(event)
                }
                Event::ConfigureNotify(event) if event.window == self.window => {
                    self.configures.push(event)
                }
                Event::ResizeRequest(event) => self.resize_requests.push(event),
                Event::MotionNotify(event) if event.event == self.window => self.motions += 1,
                Event::EnterNotify(event) if event.event == self.window => {
                    self.crossings.push(Crossing::Enter)
                }
                Event::LeaveNotify(event) if event.event == self.window => {
                    self.crossings.push(Crossing::Leave)
                }
                Event::Error(error) => panic!("X11 fixture protocol error: {error:?}"),
                _ => {}
            }
        }
    }

    pub fn clear_events(&mut self) {
        self.connection.get_input_focus().unwrap().reply().unwrap();
        self.pump();
        self.presses.clear();
        self.releases.clear();
        self.configures.clear();
        self.resize_requests.clear();
        self.motions = 0;
        self.crossings.clear();
    }

    pub fn wait_crossing(&mut self, crossing: Crossing) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.pump();
            if self.crossings.contains(&crossing) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "client did not receive X {crossing:?}Notify: {:?}",
                self.crossings
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_press(&mut self) -> ButtonPressEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.pump();
            if let Some(event) = self.presses.last() {
                return *event;
            }
            assert!(
                Instant::now() < deadline,
                "client did not receive a fresh X ButtonPress"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_release(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.pump();
            if !self.releases.is_empty() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "client did not receive X ButtonRelease"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn request(
        &mut self,
        compositor: &HeadlessCompositor,
        direction: u32,
        button: u32,
        point: (i16, i16),
    ) {
        self.request_for(compositor, self.window, direction, button, point);
    }

    pub fn request_for(
        &mut self,
        compositor: &HeadlessCompositor,
        window: Window,
        direction: u32,
        button: u32,
        point: (i16, i16),
    ) {
        self.connection.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
        self.connection
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ClientMessageEvent::new(
                    32,
                    window,
                    self.moveresize,
                    [point.0 as u32, point.1 as u32, direction, button, 1],
                ),
            )
            .unwrap();
        self.barrier(compositor);
    }

    // An X server round trip alone does not wait for the WM. A later title
    // property observed through Hearthspace proves it dispatched prior events.
    pub fn barrier(&mut self, compositor: &HeadlessCompositor) {
        self.sequence += 1;
        let title = format!("X11 CSD fixture {}", self.sequence);
        self.connection
            .change_property8(
                PropMode::REPLACE,
                self.window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .unwrap();
        self.connection.flush().unwrap();
        wait_windows(compositor, |windows| {
            windows
                .iter()
                .any(|window| window["id"] == self.id && window["title"] == title)
        });
        self.pump();
    }

    pub fn origin(&self) -> (i32, i32) {
        let origin = self
            .connection
            .translate_coordinates(self.window, self.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        (i32::from(origin.dst_x), i32::from(origin.dst_y))
    }

    pub fn overlay(&mut self, compositor: &HeadlessCompositor) -> Window {
        let window = self.connection.generate_id().unwrap();
        let x = 950 + (self.sequence % 2) as i16 * 50;
        self.connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                self.root,
                x,
                600,
                30,
                30,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new()
                    .override_redirect(1)
                    .background_pixel(0x204080),
            )
            .unwrap();
        self.connection.map_window(window).unwrap();
        self.barrier(compositor);
        wait_pixel(compositor, x as u32 + 15, 615, [32, 64, 128]);
        window
    }

    pub fn unmap(&mut self, compositor: &HeadlessCompositor, window: Window) {
        self.connection.unmap_window(window).unwrap();
        self.barrier(compositor);
    }

    pub fn destroy(&mut self, compositor: &HeadlessCompositor, window: Window) {
        self.connection.destroy_window(window).unwrap();
        self.barrier(compositor);
    }

    pub fn paint_green(&self) {
        self.connection
            .change_window_attributes(
                self.window,
                &ChangeWindowAttributesAux::new().background_pixel(0x20c060),
            )
            .unwrap();
        self.connection
            .clear_area(false, self.window, 0, 0, 0, 0)
            .unwrap();
        self.connection.flush().unwrap();
    }
}
