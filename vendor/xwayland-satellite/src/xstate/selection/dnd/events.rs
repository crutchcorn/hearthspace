use super::*;

impl XState {
    pub(crate) fn handle_drag_event(
        &mut self,
        event: &xcb::Event,
        server: &mut RealServerState,
    ) -> bool {
        let result = self.drag_x_event(event, server);
        match result {
            Ok(handled) => handled,
            Err(error) => {
                warn!("Could not complete host file drag: {error}");
                self.end_host_drag(true);
                true
            }
        }
    }

    fn drag_x_event(
        &mut self,
        event: &xcb::Event,
        server: &mut RealServerState,
    ) -> Result<bool, xcb::Error> {
        let atoms = &self.drag.atoms;
        match event {
            xcb::Event::X(x::Event::ClientMessage(message))
                if message.r#type() == atoms.status || message.r#type() == atoms.finished =>
            {
                let x::ClientMessageData::Data32(data) = message.data() else {
                    return Ok(true);
                };
                let Some(session) = self.drag.session.as_mut() else {
                    return Ok(true);
                };
                if message.window() != session.source || data[0] != session.target.resource_id() {
                    return Ok(true);
                }
                if message.r#type() == atoms.status {
                    if !session.awaiting_status || session.dropped {
                        return Ok(true);
                    }
                    session.awaiting_status = false;
                    session.accepted = data[1] & 1 != 0 && data[4] == atoms.copy.resource_id();
                    let action = if session.accepted {
                        DndAction::Copy
                    } else {
                        DndAction::empty()
                    };
                    // Wayland forbids renegotiating ordinary Copy after
                    // drop. Resolve any outstanding X status before sending
                    // XdndDrop, without changing the already-dropped offer.
                    if !session.drop_pending {
                        session.offer.set_actions(action, action);
                        session.offer.accept_mime_type(
                            session.offer.serial,
                            session.accepted.then(|| "text/uri-list".into()),
                        );
                    }
                    if let Some(point) = session.queued.take() {
                        position(&self.connection, atoms, session, point)?;
                    } else if drop_ready(&self.connection, atoms, session)? {
                        self.end_host_drag(true);
                    }
                } else if session.dropped {
                    let success = session.version < 5
                        || (data[1] & 1 != 0 && data[2] == atoms.copy.resource_id());
                    if success && session.offer.inner().is_alive() {
                        session.offer.finish();
                    }
                    self.end_host_drag(false);
                }
                Ok(true)
            }
            xcb::Event::X(x::Event::SelectionRequest(request))
                if request.selection() == atoms.selection =>
            {
                let mut accepted = false;
                if request.property() != x::ATOM_NONE {
                    if let Some(session) = self.drag.session.as_mut() {
                        // A queued request addressed to an old source must not
                        // fetch the active generation's files.
                        if request.owner() == session.source {
                            accepted = session.selection.handle_selection_request(
                                &self.connection,
                                &self.atoms,
                                request,
                                self.max_req_bytes.min(64 * 1024),
                                server,
                            );
                        }
                    }
                }
                self.connection.send_and_check_request(&x::SendEvent {
                    propagate: false,
                    destination: x::SendEventDest::Window(request.requestor()),
                    event_mask: x::EventMask::empty(),
                    event: &x::SelectionNotifyEvent::new(
                        request.time(),
                        request.requestor(),
                        request.selection(),
                        request.target(),
                        if accepted {
                            request.property()
                        } else {
                            x::ATOM_NONE
                        },
                    ),
                })?;
                Ok(true)
            }
            xcb::Event::X(x::Event::PropertyNotify(property))
                if property.state() == x::Property::Delete =>
            {
                let handled = self
                    .drag
                    .session
                    .as_mut()
                    .and_then(|session| session.selection.wayland_selection_mut())
                    .is_some_and(|selection| selection.check_for_incr(property, &self.connection));
                Ok(handled)
            }
            xcb::Event::X(x::Event::SelectionClear(clear))
                if clear.selection() == atoms.selection =>
            {
                if self
                    .drag
                    .session
                    .as_ref()
                    .is_some_and(|s| clear.owner() == s.source)
                {
                    self.end_host_drag(true);
                }
                Ok(true)
            }
            xcb::Event::X(x::Event::DestroyNotify(destroy)) => {
                if self
                    .drag
                    .session
                    .as_ref()
                    .is_some_and(|s| destroy.window() == s.target)
                {
                    self.end_host_drag(false);
                }
                Ok(false) // Ordinary window teardown still belongs to XState.
            }
            _ => Ok(false),
        }
    }
}
