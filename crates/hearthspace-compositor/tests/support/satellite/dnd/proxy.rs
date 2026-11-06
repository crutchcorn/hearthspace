//! Real InputOnly proxy windows on the destination's X connection.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{CreateWindowAux, WindowClass};

impl Receiver {
    pub fn install_proxy(&self, valid: bool, hide_target_aware: bool) -> u32 {
        let connection = &self.client.connection;
        let proxy = connection.generate_id().unwrap();
        connection
            .create_window(
                0,
                proxy,
                self.client.window,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap()
            .check()
            .unwrap();
        let atom = connection
            .intern_atom(false, b"XdndProxy")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(
                PropMode::REPLACE,
                self.client.window,
                atom,
                AtomEnum::WINDOW,
                &[proxy],
            )
            .unwrap()
            .check()
            .unwrap();
        connection
            .change_property32(
                PropMode::REPLACE,
                proxy,
                atom,
                AtomEnum::WINDOW,
                &[if valid { proxy } else { self.client.window }],
            )
            .unwrap()
            .check()
            .unwrap();
        let aware = connection
            .intern_atom(false, b"XdndAware")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(PropMode::REPLACE, proxy, aware, AtomEnum::ATOM, &[5])
            .unwrap()
            .check()
            .unwrap();
        if hide_target_aware {
            connection
                .delete_property(self.client.window, aware)
                .unwrap()
                .check()
                .unwrap();
        }
        proxy
    }

    pub fn clear_proxy(&self) {
        let connection = &self.client.connection;
        let proxy = connection
            .intern_atom(false, b"XdndProxy")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .delete_property(self.client.window, proxy)
            .unwrap()
            .check()
            .unwrap();
        let aware = connection
            .intern_atom(false, b"XdndAware")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(
                PropMode::REPLACE,
                self.client.window,
                aware,
                AtomEnum::ATOM,
                &[5],
            )
            .unwrap()
            .check()
            .unwrap();
    }
}
