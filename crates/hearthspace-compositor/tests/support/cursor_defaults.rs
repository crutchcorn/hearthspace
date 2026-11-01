//! Real Xwayland/libXcursor preference resolution, independent of host themes.
use super::{
    navigation::{action, query},
    *,
};
use serde_json::Value;
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt, PropMode, Window},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};
use std::fs;

// Exercise the actual initializer against an already populated owned server as
// well as through App startup. No test-only control command or XWM API is needed.
#[path = "../../src/compositor/xwayland/cursor_defaults.rs"]
mod initialization;

struct Fixture {
    compositor: HeadlessCompositor,
    home: tempfile::TempDir,
    display: String,
    connection: RustConnection,
    root: Window,
}

impl Fixture {
    fn start(legacy: Option<(&str, &[u8])>) -> Self {
        let home = tempfile::tempdir().unwrap();
        if let Some((name, contents)) = legacy {
            fs::write(home.path().join(name), contents).unwrap();
        }
        let mut compositor =
            HeadlessCompositor::spawn_with_command("800x600", None, None, 1, false, |command| {
                command
                    .env("HOME", home.path())
                    .env("XDG_CONFIG_HOME", home.path().join("config"))
                    .env_remove("DISPLAY")
                    .env_remove("XAUTHORITY")
                    .env_remove("XCURSOR_SIZE")
                    .env_remove("XCURSOR_THEME")
                    .env_remove("XENVIRONMENT")
                    .env_remove(hearthspace_desktop::activation::MARKER)
                    .env_remove(hearthspace_common::SESSION_DIR_ENV)
                    .env_remove("HEARTHSPACE_SETTINGS_SOCKET")
                    .env_remove("HEARTHSPACE_LOG_FILE")
                    .env_remove("DBUS_SESSION_BUS_ADDRESS")
                    .env_remove("DBUS_SYSTEM_BUS_ADDRESS")
                    .env_remove("AT_SPI_BUS_ADDRESS");
            });
        drop(compositor.wait_for_socket());
        let deadline = Instant::now() + Duration::from_secs(10);
        let display = loop {
            if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
                break display.to_owned();
            }
            assert!(
                Instant::now() < deadline,
                "owned Xwayland did not become ready"
            );
            thread::sleep(Duration::from_millis(20));
        };
        let (connection, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen].root;
        Self {
            compositor,
            home,
            display,
            connection,
            root,
        }
    }

    fn resources(&self) -> Option<Vec<u8>> {
        let reply = self
            .connection
            .get_property(
                false,
                self.root,
                AtomEnum::RESOURCE_MANAGER,
                AtomEnum::ANY,
                0,
                4096,
            )
            .unwrap()
            .reply()
            .unwrap();
        if reply.type_ == x11rb::NONE {
            return None;
        }
        assert_eq!(reply.type_, u32::from(AtomEnum::STRING));
        assert_eq!(reply.format, 8);
        assert_eq!(reply.bytes_after, 0, "read the complete fixture database");
        Some(reply.value)
    }

    fn set_resources(&self, contents: &[u8]) {
        self.connection
            .change_property8(
                PropMode::REPLACE,
                self.root,
                AtomEnum::RESOURCE_MANAGER,
                AtomEnum::STRING,
                contents,
            )
            .unwrap()
            .check()
            .unwrap();
    }

    fn initialize_again(&self) {
        let number = self.display.strip_prefix(':').unwrap().parse().unwrap();
        initialization::initialize(number, Some(self.home.path()), 24).unwrap();
    }

    fn probe(&self, name: &str, size: Option<&str>) -> Value {
        let root = self.compositor.runtime_dir.path();
        let script = root.join("xcursor-probe.py");
        fs::write(&script, include_str!("cursor_defaults/probe.py")).unwrap();
        let output_path = root.join(format!("{name}.json"));
        let error_path = root.join(format!("{name}.stderr"));
        let mut command = Command::new("python3");
        command
            .arg("-I")
            .arg(&script)
            .arg(&self.display)
            .env("HOME", self.home.path())
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .env_remove("XCURSOR_SIZE")
            .env_remove("XCURSOR_THEME")
            .env_remove("XENVIRONMENT")
            .stdin(Stdio::null())
            .stdout(fs::File::create(&output_path).unwrap())
            .stderr(fs::File::create(&error_path).unwrap());
        if let Some(size) = size {
            command.env("XCURSOR_SIZE", size);
        }
        let mut probe = Probe(command.spawn().expect("start the Python libXcursor probe"));
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = probe.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "libXcursor probe timed out");
            thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "libXcursor probe failed ({status}); requires Python, libX11.so.6 and libXcursor.so.1:\n{}",
            fs::read_to_string(error_path).unwrap()
        );
        serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap()
    }

    fn finish(mut self) {
        action(&self.compositor, "quit");
        wait_for_exit(&mut self.compositor.child);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if thread::panicking() {
            self.home.disable_cleanup(true);
            eprintln!("Xcursor fixture HOME: {}", self.home.path().display());
        }
    }
}

struct Probe(Child);

impl Drop for Probe {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn libxcursor_gets_default_at_display_readiness_and_keeps_environment_override() {
    if std::env::var_os("HEARTHSPACE_E2E").is_none() {
        return;
    }
    let _guard = headless_test_guard();
    let fixture = Fixture::start(None);
    let baseline = fixture.probe("default", None);
    assert_eq!(baseline["requested_size"], 24);
    assert_eq!(baseline["resource_size"], "24");
    assert_eq!(baseline["screen_size"], serde_json::json!([800, 600]));
    // Before this fix, the real library requests min(800,600)/48 == 12.
    // An explicit per-client preference must still outrank the server default.
    let resources = fixture.resources();
    let overridden = fixture.probe("environment-override", Some("48"));
    assert_eq!(overridden["requested_size"], 48);
    assert_eq!(overridden["resource_size"], "24");
    assert_eq!(fixture.resources(), resources);
    fixture.finish();
}

#[test]
fn libxcursor_keeps_existing_resource_preferences_and_empty_property() {
    if std::env::var_os("HEARTHSPACE_E2E").is_none() {
        return;
    }
    let _guard = headless_test_guard();
    let fixture = Fixture::start(None);
    let resources = b"Xcursor*size: 40\nXft.dpi: 144\nTest.sentinel: keep me\n";
    fixture.set_resources(resources);
    fixture.initialize_again();
    assert_eq!(fixture.resources().as_deref(), Some(resources.as_slice()));
    assert_eq!(
        fixture.probe("resource-override", None)["requested_size"],
        40
    );
    assert_eq!(
        fixture.probe("environment-over-resource", Some("48"))["requested_size"],
        48
    );
    fixture.set_resources(b"");
    fixture.initialize_again();
    assert_eq!(fixture.resources(), Some(Vec::new()));
    assert_eq!(fixture.probe("empty-resource", None)["requested_size"], 12);
    fixture.finish();
}

#[test]
fn libxcursor_preserves_legacy_xdefaults_instead_of_masking_it() {
    if std::env::var_os("HEARTHSPACE_E2E").is_none() {
        return;
    }
    let _guard = headless_test_guard();
    let resources = b"Xcursor.size: 40\nTest.sentinel: keep me\n";
    let fixture = Fixture::start(Some((".Xdefaults", resources)));
    assert_eq!(fixture.resources(), None);
    assert_eq!(fixture.probe("legacy-defaults", None)["requested_size"], 40);
    assert_eq!(
        fs::read(fixture.home.path().join(".Xdefaults"))
            .unwrap()
            .as_slice(),
        resources.as_slice()
    );
    fixture.finish();
}

#[test]
fn legacy_xresources_is_preserved_even_without_a_cursor_preference() {
    if std::env::var_os("HEARTHSPACE_E2E").is_none() {
        return;
    }
    let _guard = headless_test_guard();
    let resources = b"Test.sentinel: preserve this database for XCB clients\n";
    let fixture = Fixture::start(Some((".Xresources", resources)));
    assert_eq!(fixture.resources(), None);
    assert_eq!(
        fs::read(fixture.home.path().join(".Xresources"))
            .unwrap()
            .as_slice(),
        resources.as_slice()
    );
    // Xlib does not read .Xresources itself. The conservative opt-out leaves its
    // original display-size fallback intact rather than masking user databases.
    assert_eq!(
        fixture.probe("legacy-resources", None)["requested_size"],
        12
    );
    fixture.finish();
}
