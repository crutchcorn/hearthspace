//! Minimal non-reparenting XWM: manual redirection and legacy surface pairing.
//! Smithay's X11Wm requires its own launcher/client data and shared X sockets.
use anyhow::{Result, bail};
use std::{collections::HashMap, os::unix::net::UnixStream};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        composite::{ConnectionExt as _, Redirect},
        xproto::{
            Atom, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux,
            EventMask, StackMode, WindowClass,
        },
    },
    rust_connection::{DefaultStream, RustConnection},
};

pub fn connection(stream: UnixStream) -> Result<RustConnection> {
    Ok(RustConnection::connect_to_stream(
        DefaultStream::from_unix_stream(stream)?.0,
        0,
    )?)
}

pub struct Wm {
    connection: RustConnection,
    surface_atom: Atom,
    pub surfaces: HashMap<u32, u32>,
}

impl Wm {
    pub fn connect(stream: UnixStream) -> Result<Self> {
        let c = connection(stream)?;
        let root = c.setup().roots[0].root;
        c.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY),
        )?
        .check()?;
        c.composite_query_version(0, 4)?.reply()?;
        c.composite_redirect_subwindows(root, Redirect::MANUAL)?
            .check()?;
        let window = c.generate_id()?;
        c.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )?
        .check()?;
        for name in [b"WM_S0".as_slice(), b"_NET_WM_CM_S0".as_slice()] {
            let atom = c.intern_atom(false, name)?.reply()?.atom;
            c.set_selection_owner(window, atom, x11rb::CURRENT_TIME)?
                .check()?;
        }
        let surface_atom = c.intern_atom(false, b"WL_SURFACE_ID")?.reply()?.atom;
        Ok(Self {
            connection: c,
            surface_atom,
            surfaces: HashMap::new(),
        })
    }

    pub fn pump(&mut self) -> Result<()> {
        while let Some(event) = self.connection.poll_for_event()? {
            match event {
                Event::ClientMessage(e) if e.type_ == self.surface_atom && e.format == 32 => {
                    self.surfaces.insert(e.window, e.data.as_data32()[0]);
                }
                Event::MapRequest(e) => {
                    self.connection.map_window(e.window)?;
                }
                Event::DestroyNotify(e) => {
                    self.surfaces.remove(&e.window);
                }
                Event::Error(e) => bail!("XWM protocol error: {e:?}"),
                _ => {}
            }
        }
        self.connection.flush()?;
        Ok(())
    }

    pub fn raise(&self, window: u32) -> Result<()> {
        // Only this XWM connection may bypass its own SubstructureRedirect.
        self.connection
            .configure_window(
                window,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?
            .check()?;
        Ok(())
    }
}
