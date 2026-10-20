//! X11 clients must receive appearance changes without unrelated XWM traffic.
use super::navigation::{action, query};
use super::*;
use hearthspace_ipc::{ColorMode, transport::request_line};
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{Atom, ChangeWindowAttributesAux, ConnectionExt, EventMask, Window},
    },
    rust_connection::RustConnection,
};

struct SettingsDaemon {
    child: Child,
    directory: tempfile::TempDir,
    socket: PathBuf,
}

impl SettingsDaemon {
    fn spawn() -> Self {
        let binary = std::env::var_os("HEARTHSPACE_TEST_SETTINGSD_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_BIN_EXE_hearthspace")).with_file_name("hearthspace-settingsd")
            });
        assert!(
            binary.is_file(),
            "XSettings E2E requires the real settings daemon: {}. Build hearthspace-settingsd \
             with the workspace binaries or set HEARTHSPACE_TEST_SETTINGSD_BINARY.",
            binary.display()
        );
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("settings.sock");
        let log = std::fs::File::create(directory.path().join("settingsd.log")).unwrap();
        let child = Command::new(binary)
            .env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket)
            .env("XDG_RUNTIME_DIR", directory.path())
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("start the private settings daemon");
        let mut daemon = Self {
            child,
            directory,
            socket,
        };
        drop(
            startup::wait_for_socket(&mut daemon.child, &daemon.socket)
                .expect("private settings daemon readiness"),
        );
        daemon
    }

    fn set_mode(&self, mode: ColorMode) {
        let reply = request_line(
            &self.socket,
            &format!("set color-mode {mode}\n"),
            Duration::from_secs(2),
        )
        .expect("save appearance through settingsd");
        assert!(reply.starts_with("ok "), "settings save failed: {reply}");
        assert_eq!(
            hearthspace_ipc::client_get_settings_at(&self.socket)
                .unwrap()
                .appearance
                .color_mode,
            mode,
            "the private daemon must persist the requested mode"
        );
    }
}

impl Drop for SettingsDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking() {
            self.directory.disable_cleanup(true);
            eprintln!(
                "XSettings settingsd artifacts: {}",
                self.directory.path().display()
            );
        }
    }
}

struct Observer {
    connection: RustConnection,
    owner: Window,
    property: Atom,
}

impl Observer {
    fn connect(display: &str) -> Self {
        let (connection, screen) = x11rb::connect(Some(display)).expect("connect X11 observer");
        let selection = connection
            .intern_atom(false, format!("_XSETTINGS_S{screen}").as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let property = connection
            .intern_atom(false, b"_XSETTINGS_SETTINGS")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let deadline = Instant::now() + Duration::from_secs(5);
        let owner = loop {
            let owner = connection
                .get_selection_owner(selection)
                .unwrap()
                .reply()
                .unwrap()
                .owner;
            if owner != x11rb::NONE {
                break owner;
            }
            assert!(
                Instant::now() < deadline,
                "XSettings selection has no owner"
            );
            thread::sleep(Duration::from_millis(10));
        };
        connection
            .change_window_attributes(
                owner,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .unwrap()
            .check()
            .unwrap();
        Self {
            connection,
            owner,
            property,
        }
    }

    fn theme(&self) -> String {
        let reply = self
            .connection
            .get_property(false, self.owner, self.property, self.property, 0, 4096)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(reply.format, 8, "XSettings property must contain bytes");
        assert_eq!(reply.bytes_after, 0, "read the complete XSettings property");
        theme_name(&reply.value)
    }

    fn settle(&self) {
        // Complete this observer's subscription/read requests before changing
        // settings. The compositor has no X11 clients/windows to generate events.
        self.connection.get_input_focus().unwrap().reply().unwrap();
        while self.connection.poll_for_event().unwrap().is_some() {}
    }

    fn wait_for_change_without_requests(&self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            // poll_for_event only receives: no property reads, sync round trips,
            // mapped windows, input, or any request that could wake the XWM.
            if let Some(Event::PropertyNotify(event)) = self
                .connection
                .poll_for_event()
                .expect("receive XSettings notification")
                && event.window == self.owner
                && event.atom == self.property
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "appearance change was not delivered to the idle X11 observer; \
                 XSettings publication must flush without unrelated XWM events"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn theme_name(bytes: &[u8]) -> String {
    assert!(bytes.len() >= 12, "complete XSettings header");
    let little = bytes[0] == 0;
    assert!(bytes[0] <= 1, "XSettings byte order");
    let word = |offset: usize| {
        let data = bytes[offset..offset + 4].try_into().unwrap();
        if little {
            u32::from_le_bytes(data)
        } else {
            u32::from_be_bytes(data)
        }
    };
    let mut offset = 12;
    for _ in 0..word(8) {
        let kind = bytes[offset];
        let length = bytes[offset + 2..offset + 4].try_into().unwrap();
        let length = if little {
            u16::from_le_bytes(length)
        } else {
            u16::from_be_bytes(length)
        } as usize;
        offset += 4;
        let name = &bytes[offset..offset + length];
        offset = (offset + length).next_multiple_of(4) + 4; // name padding + change serial
        match kind {
            0 => offset += 4,
            1 => {
                let length = word(offset) as usize;
                offset += 4;
                let value = &bytes[offset..offset + length];
                if name == b"Net/ThemeName" {
                    return String::from_utf8(value.to_vec()).unwrap();
                }
                offset = (offset + length).next_multiple_of(4);
            }
            2 => offset += 8,
            _ => panic!("unknown XSettings value type {kind}"),
        }
    }
    panic!("Net/ThemeName missing from XSettings property");
}

#[test]
fn xsettings_publishes_saved_startup_theme_and_idle_live_changes() {
    if std::env::var_os("HEARTHSPACE_E2E").is_none() {
        return;
    }
    let _guard = headless_test_guard();
    let settings = SettingsDaemon::spawn();
    settings.set_mode(ColorMode::Dark);
    let mut compositor =
        HeadlessCompositor::spawn_with_command("800x600", None, None, 1, false, |command| {
            command
                .env(hearthspace_common::SETTINGS_SOCKET_ENV, &settings.socket)
                .env("XDG_CONFIG_HOME", settings.directory.path().join("config"));
        });
    drop(compositor.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        let status = query(&compositor, "status");
        if let Some(display) = status["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline, "XWayland did not start");
        thread::sleep(Duration::from_millis(25));
    };
    let observer = Observer::connect(&display);
    assert_eq!(observer.theme(), "Adwaita-dark", "saved theme at startup");
    observer.settle();

    settings.set_mode(ColorMode::Light);
    action(&compositor, "reload-settings");
    observer.wait_for_change_without_requests();
    assert_eq!(observer.theme(), "Adwaita");
    observer.settle();

    settings.set_mode(ColorMode::Dark);
    action(&compositor, "reload-settings");
    observer.wait_for_change_without_requests();
    assert_eq!(observer.theme(), "Adwaita-dark");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
