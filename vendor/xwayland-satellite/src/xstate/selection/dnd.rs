//! Copy-only host URI drags. Each drag owns a distinct X selection window.
mod events;
mod proxy;
use super::{
    Clipboard, CurrentSelection, ForeignSelection, SelectionData, SelectionDataImpl,
    SelectionTargetId, WaylandSelection,
};
use crate::{RealServerState, server::drag::Event, xstate::XState};
use log::warn;
use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;
use wayland_client::{Proxy, protocol::wl_data_device_manager::DndAction};
use xcb::{Xid, x};

xcb::atoms_struct! {
    #[derive(Clone)]
    struct Atoms {
        aware => b"XdndAware" only_if_exists = false,
        proxy => b"XdndProxy" only_if_exists = false,
        enter => b"XdndEnter" only_if_exists = false,
        position => b"XdndPosition" only_if_exists = false,
        status => b"XdndStatus" only_if_exists = false,
        leave => b"XdndLeave" only_if_exists = false,
        drop => b"XdndDrop" only_if_exists = false,
        finished => b"XdndFinished" only_if_exists = false,
        selection => b"XdndSelection" only_if_exists = false,
        copy => b"XdndActionCopy" only_if_exists = false,
        types => b"XdndTypeList" only_if_exists = false,
        uri => b"text/uri-list" only_if_exists = false,
    }
}

pub(crate) struct State {
    atoms: Atoms,
    session: Option<Session>,
}
impl State {
    pub(crate) fn new(connection: &xcb::Connection) -> Self {
        Self {
            atoms: Atoms::intern_all(connection).unwrap(),
            session: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Position {
    packed: u32,
    time: u32,
}
impl Position {
    fn new(offer: &DragOffer, scale: f64, origin: (i16, i16)) -> Self {
        let x = (f64::from(origin.0) + offer.x * scale).round() as i16 as u16;
        let y = (f64::from(origin.1) + offer.y * scale).round() as i16 as u16;
        Self {
            packed: (u32::from(x) << 16) | u32::from(y),
            time: offer.time.unwrap_or(x::CURRENT_TIME),
        }
    }
}

struct Session {
    source: x::Window,
    target: x::Window,
    delivery: x::Window,
    version: u32,
    offer: DragOffer,
    selection: SelectionData<Clipboard>,
    position: Position,
    queued: Option<Position>,
    awaiting_status: bool,
    accepted: bool,
    drop_pending: bool,
    dropped: bool,
}

fn send(
    connection: &xcb::Connection,
    destination: x::Window,
    target: x::Window,
    atom: x::Atom,
    data: [u32; 5],
) -> Result<(), xcb::Error> {
    Ok(connection.send_and_check_request(&x::SendEvent {
        propagate: false,
        destination: x::SendEventDest::Window(destination),
        event_mask: x::EventMask::empty(),
        event: &x::ClientMessageEvent::new(target, atom, x::ClientMessageData::Data32(data)),
    })?)
}

fn position(
    connection: &xcb::Connection,
    atoms: &Atoms,
    session: &mut Session,
    point: Position,
) -> Result<(), xcb::Error> {
    if session.awaiting_status {
        session.queued = Some(point);
        return Ok(());
    }
    session.position = point;
    session.awaiting_status = true;
    send(
        connection,
        session.delivery,
        session.target,
        atoms.position,
        [
            session.source.resource_id(),
            0,
            point.packed,
            point.time,
            atoms.copy.resource_id(),
        ],
    )
}

/// Returns true when a rejected pending drop should retire the session.
fn drop_ready(
    connection: &xcb::Connection,
    atoms: &Atoms,
    session: &mut Session,
) -> Result<bool, xcb::Error> {
    if !session.drop_pending || session.awaiting_status || session.dropped {
        return Ok(false);
    }
    if !session.accepted {
        return Ok(true);
    }
    send(
        connection,
        session.delivery,
        session.target,
        atoms.drop,
        [session.source.resource_id(), 0, session.position.time, 0, 0],
    )?;
    session.dropped = true;
    Ok(false)
}

impl XState {
    pub(crate) fn handle_host_drag(&mut self, server: &mut RealServerState) {
        for event in server.take_drag_events() {
            if let Err(error) = self.host_drag_event(event, server) {
                warn!("Could not bridge host file drag: {error}");
                self.end_host_drag(true);
            }
        }
    }

    fn host_drag_event(
        &mut self,
        event: Event,
        server: &RealServerState,
    ) -> Result<(), xcb::Error> {
        match event {
            Event::Enter(offer) => {
                self.end_host_drag(true);
                let Some((target, scale, origin)) = server.drag_target(&offer) else {
                    return Ok(());
                };
                let uri =
                    offer.with_mime_types(|mimes| mimes.iter().any(|mime| mime == "text/uri-list"));
                if !uri || !offer.source_actions.contains(DndAction::Copy) {
                    return Ok(());
                }
                let atoms = &self.drag.atoms;
                let delivery = proxy::resolve(&self.connection, atoms, target);
                let aware = self
                    .connection
                    .wait_for_reply(self.connection.send_request(&x::GetProperty {
                        delete: false,
                        window: delivery,
                        property: atoms.aware,
                        r#type: x::ATOM_ATOM,
                        long_offset: 0,
                        long_length: 1,
                    }))?;
                if aware.format() != 32 {
                    return Ok(());
                }
                let Some(&version) = aware.value::<u32>().first() else {
                    return Ok(());
                };
                if version < 2 {
                    return Ok(());
                }
                let version = version.min(5);
                let source = self.connection.generate_id();
                self.connection.send_and_check_request(&x::CreateWindow {
                    depth: 0,
                    wid: source,
                    parent: self.root,
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    border_width: 0,
                    class: x::WindowClass::InputOnly,
                    visual: x::COPY_FROM_PARENT,
                    value_list: &[],
                })?;
                let mut selection = SelectionData::new(atoms.selection, self.atoms.targets);
                selection.current_selection = Some(CurrentSelection::Wayland(WaylandSelection {
                    mimes: vec![SelectionTargetId {
                        name: "text/uri-list".into(),
                        target: atoms.uri,
                        property: atoms.uri,
                        source: None,
                    }],
                    inner: ForeignSelection::from_drag(offer.clone()),
                    incr_data: None,
                }));
                let point = Position::new(&offer, scale, origin);
                self.drag.session = Some(Session {
                    source,
                    target,
                    delivery,
                    version,
                    offer,
                    selection,
                    position: point,
                    queued: None,
                    awaiting_status: false,
                    accepted: false,
                    drop_pending: false,
                    dropped: false,
                });
                let session = self.drag.session.as_ref().unwrap();
                if !session.selection.set_owner(&self.connection, source) {
                    self.end_host_drag(false);
                    return Ok(());
                }
                self.connection.send_and_check_request(&x::ChangeProperty {
                    mode: x::PropMode::Replace,
                    window: source,
                    property: atoms.types,
                    r#type: x::ATOM_ATOM,
                    data: &[atoms.uri],
                })?;
                send(
                    &self.connection,
                    delivery,
                    target,
                    atoms.enter,
                    [
                        source.resource_id(),
                        version << 24,
                        atoms.uri.resource_id(),
                        0,
                        0,
                    ],
                )?;
                position(
                    &self.connection,
                    &self.drag.atoms,
                    self.drag.session.as_mut().unwrap(),
                    point,
                )?;
            }
            Event::Motion(offer) => {
                let Some((target, scale, origin)) = server.drag_target(&offer) else {
                    return Ok(());
                };
                let Some(session) = self.drag.session.as_mut() else {
                    return Ok(());
                };
                if session.target == target && session.offer == offer && !session.drop_pending {
                    position(
                        &self.connection,
                        &self.drag.atoms,
                        session,
                        Position::new(&offer, scale, origin),
                    )?;
                }
            }
            Event::Drop(offer) => {
                let Some(session) = self.drag.session.as_mut() else {
                    return Ok(());
                };
                if session.offer != offer || session.dropped {
                    return Ok(());
                }
                session.offer = offer.clone();
                if let Some(selection) = session.selection.wayland_selection_mut() {
                    selection.inner = ForeignSelection::from_drag(offer);
                }
                session.drop_pending = true;
                if drop_ready(&self.connection, &self.drag.atoms, session)? {
                    self.end_host_drag(true);
                }
            }
            Event::Leave => {
                // Leave follows drop, too. A dropped offer remains valid until
                // its exact X target finishes; an ordinary leave cancels it.
                if self.drag.session.as_ref().is_some_and(|s| !s.drop_pending) {
                    self.end_host_drag(true);
                }
            }
        }
        Ok(())
    }

    fn end_host_drag(&mut self, leave: bool) {
        let Some(session) = self.drag.session.take() else {
            return;
        };
        if leave && !session.dropped {
            let _ = send(
                &self.connection,
                session.delivery,
                session.target,
                self.drag.atoms.leave,
                [session.source.resource_id(), 0, 0, 0, 0],
            );
        }
        if session.offer.inner().is_alive() {
            if session.drop_pending {
                session.offer.destroy();
            } else {
                // SCTK owns undropped offers until host leave. Revoke the
                // acceptance now; destroying it alone leaves the host's shared
                // offer state accepted and can race SCTK's leave cleanup.
                session.offer.accept_mime_type(session.offer.serial, None);
                session
                    .offer
                    .set_actions(DndAction::empty(), DndAction::empty());
            }
        }
        // Destroying this incarnation clears selection ownership only if it
        // still owns it; it cannot clear a newer client/drag's claim.
        let _ = self.connection.send_and_check_request(&x::DestroyWindow {
            window: session.source,
        });
    }
}
