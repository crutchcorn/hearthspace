//! An actual XDND destination, including property-delete INCR acknowledgments.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{
    ClientMessageEvent, Property, SelectionNotifyEvent,
};

pub(super) struct Receiver {
    pub client: WindowClient,
    pub accept: bool,
    pub sources: Vec<u32>,
    pub positions: usize,
    pub leaves: usize,
    pub dropped: Vec<Vec<u8>>,
    pub incr_transfers: usize,
    pub refused: usize,
    atoms: Atoms,
    source: Option<u32>,
    bytes: Vec<u8>,
    incr: bool,
    barrier: u64,
}

struct Atoms {
    enter: u32,
    position: u32,
    status: u32,
    drop: u32,
    leave: u32,
    finished: u32,
    selection: u32,
    copy: u32,
    uri: u32,
    property: u32,
    incr: u32,
}

impl Receiver {
    pub fn new(compositor: &HeadlessCompositor, display: &str) -> Self {
        let client = WindowClient::new(compositor, display, "X11 file receiver", 0x204080);
        Self::from_client(client)
    }

    pub fn from_client(client: WindowClient) -> Self {
        let atom = |name: &[u8]| {
            client
                .connection
                .intern_atom(false, name)
                .unwrap()
                .reply()
                .unwrap()
                .atom
        };
        let aware = atom(b"XdndAware");
        let atoms = Atoms {
            enter: atom(b"XdndEnter"),
            position: atom(b"XdndPosition"),
            status: atom(b"XdndStatus"),
            drop: atom(b"XdndDrop"),
            leave: atom(b"XdndLeave"),
            finished: atom(b"XdndFinished"),
            selection: atom(b"XdndSelection"),
            copy: atom(b"XdndActionCopy"),
            uri: atom(b"text/uri-list"),
            property: atom(b"HEARTHSPACE_DROP"),
            incr: atom(b"INCR"),
        };
        client
            .connection
            .change_property32(
                PropMode::REPLACE,
                client.window,
                aware,
                AtomEnum::ATOM,
                &[5],
            )
            .unwrap()
            .check()
            .unwrap();
        client
            .connection
            .change_window_attributes(
                client.window,
                &smithay::reexports::x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                    .event_mask(
                        EventMask::BUTTON_PRESS
                            | EventMask::BUTTON_RELEASE
                            | EventMask::PROPERTY_CHANGE,
                    ),
            )
            .unwrap()
            .check()
            .unwrap();
        Self {
            client,
            accept: true,
            sources: Vec::new(),
            positions: 0,
            leaves: 0,
            dropped: Vec::new(),
            incr_transfers: 0,
            refused: 0,
            atoms,
            source: None,
            bytes: Vec::new(),
            incr: false,
            barrier: 0,
        }
    }

    fn send(&self, destination: u32, window: u32, atom: u32, data: [u32; 5]) {
        self.client
            .connection
            .send_event(
                false,
                destination,
                EventMask::NO_EVENT,
                ClientMessageEvent::new(32, window, atom, data),
            )
            .unwrap()
            .check()
            .unwrap();
    }

    pub fn stale_status(&self, old: u32) {
        let current = self.source.unwrap();
        // Replay the fields of an old queued Status to the still-live bridge
        // connection, after that source incarnation has been destroyed.
        self.send(
            current,
            old,
            self.atoms.status,
            [self.client.window, 3, 0, 0, self.atoms.copy],
        );
        self.send(
            current,
            old,
            self.atoms.finished,
            [self.client.window, 1, self.atoms.copy, 0, 0],
        );
    }

    pub fn stale_conversion(&self, old: u32) {
        use smithay::reexports::x11rb::protocol::xproto::{
            SELECTION_REQUEST_EVENT, SelectionRequestEvent,
        };
        self.client
            .connection
            .send_event(
                false,
                self.source.unwrap(),
                EventMask::NO_EVENT,
                SelectionRequestEvent {
                    response_type: SELECTION_REQUEST_EVENT,
                    sequence: 0,
                    time: x11rb::CURRENT_TIME,
                    owner: old,
                    requestor: self.client.window,
                    selection: self.atoms.selection,
                    target: self.atoms.uri,
                    property: self.atoms.property,
                },
            )
            .unwrap()
            .check()
            .unwrap();
    }

    pub fn barrier(&mut self, compositor: &HeadlessCompositor) {
        self.barrier += 1;
        let title = format!("X11 file receiver {}", self.barrier);
        self.client
            .connection
            .change_property8(
                PropMode::REPLACE,
                self.client.window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .unwrap();
        self.client.connection.flush().unwrap();
        // X Status precedes this property event on Satellite's connection.
        // Its Wayland accept/actions precede this XDG title on the host socket.
        wait_windows(compositor, |windows| {
            windows
                .iter()
                .any(|w| w["id"] == self.client.id && w["title"] == title)
        });
        self.pump();
    }

    pub fn wait_owner_released(&mut self) {
        let source = self.source.unwrap();
        self.wait(|r| {
            r.client
                .connection
                .get_selection_owner(r.atoms.selection)
                .unwrap()
                .reply()
                .unwrap()
                .owner
                != source
        });
    }

    pub fn wait(&mut self, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.pump();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "XDND incomplete: positions={} leaves={} drops={} refused={}",
                self.positions,
                self.leaves,
                self.dropped.len(),
                self.refused
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn converted(&mut self, event: SelectionNotifyEvent) {
        assert_eq!(event.selection, self.atoms.selection);
        if event.property == x11rb::NONE {
            self.refused += 1;
            return;
        }
        assert_eq!(event.property, self.atoms.property);
        self.read_property();
    }

    fn read_property(&mut self) {
        let reply = self
            .client
            .connection
            .get_property(
                true,
                self.client.window,
                self.atoms.property,
                AtomEnum::ANY,
                0,
                u32::MAX,
            )
            .unwrap()
            .reply()
            .unwrap();
        if reply.type_ == self.atoms.incr {
            self.incr = true;
            self.incr_transfers += 1;
            self.bytes.clear();
        } else {
            assert_eq!(reply.type_, self.atoms.uri);
            assert_eq!(reply.format, 8);
            if self.incr {
                if reply.value.is_empty() {
                    self.complete();
                } else {
                    self.bytes.extend(reply.value);
                }
            } else {
                self.bytes = reply.value;
                self.complete();
            }
        }
        self.client.connection.flush().unwrap();
    }

    fn complete(&mut self) {
        self.dropped.push(std::mem::take(&mut self.bytes));
        self.incr = false;
        let source = self.source.unwrap();
        self.send(
            source,
            source,
            self.atoms.finished,
            [self.client.window, 1, self.atoms.copy, 0, 0],
        );
    }

    pub fn pump(&mut self) {
        while let Some(event) = self.client.connection.poll_for_event().unwrap() {
            match event {
                Event::ClientMessage(message) => {
                    assert_eq!(message.window, self.client.window);
                    let data = message.data.as_data32();
                    if message.type_ == self.atoms.enter {
                        assert_eq!(data[1] >> 24, 5);
                        assert_eq!(data[1] & 1, 0);
                        assert!(data[2..].contains(&self.atoms.uri));
                        self.source = Some(data[0]);
                        self.sources.push(data[0]);
                    } else if message.type_ == self.atoms.position {
                        assert_eq!(self.source, Some(data[0]));
                        let origin = self.client.origin();
                        let expected = (((origin.0 + 60) as u16 as u32) << 16)
                            | u32::from((origin.1 + 40) as u16);
                        assert_eq!(
                            data[2], expected,
                            "XDND root point must equal backing origin {origin:?} plus client-local (60,40)"
                        );
                        assert_eq!(data[4], self.atoms.copy);
                        self.positions += 1;
                        self.send(
                            data[0],
                            data[0],
                            self.atoms.status,
                            [
                                self.client.window,
                                if self.accept { 3 } else { 2 },
                                0,
                                0,
                                if self.accept { self.atoms.copy } else { 0 },
                            ],
                        );
                    } else if message.type_ == self.atoms.drop {
                        assert_eq!(self.source, Some(data[0]));
                        assert!(self.accept, "rejected target received a drop");
                        self.client
                            .connection
                            .convert_selection(
                                self.client.window,
                                self.atoms.selection,
                                self.atoms.uri,
                                self.atoms.property,
                                data[2],
                            )
                            .unwrap();
                        self.client.connection.flush().unwrap();
                    } else if message.type_ == self.atoms.leave {
                        self.leaves += 1;
                    } else {
                        panic!("unexpected X11 message: {message:?}");
                    }
                }
                Event::SelectionNotify(event) => self.converted(event),
                Event::PropertyNotify(event)
                    if self.incr
                        && event.atom == self.atoms.property
                        && event.state == Property::NEW_VALUE =>
                {
                    self.read_property()
                }
                Event::ButtonPress(event) | Event::ButtonRelease(event) => {
                    panic!("desktop drag clicked application: {event:?}")
                }
                Event::Error(error) => panic!("XDND client protocol error: {error:?}"),
                _ => {}
            }
        }
    }
}
