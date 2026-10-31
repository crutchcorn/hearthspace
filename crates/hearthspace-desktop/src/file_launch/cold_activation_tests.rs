//! A real private broker has a cold activatable service. Scoped startup must use
//! the prepared process instead; native startup must still activate that service.
use super::test_support::*;
use super::*;
use std::{
    collections::HashMap,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    time::Instant,
};

mod gpu;
mod ordinary;

const APP: &str = "org.hearthspace.ColdPlayer";
const LAUNCH: &str = "file_launch::cold_activation_tests::launch_in_isolated_environment";
const HELPER: &str = "file_launch::cold_activation_tests::gio_helper_in_isolated_environment";
const SERVICE: &str = "file_launch::cold_activation_tests::serve_cold_player";

// Release the broker-started test service before Fixture deletes its directory,
// including when a delivery assertion fails. The service also has its own bound.
struct ServiceLifetime<'a>(&'a Path);

impl Drop for ServiceLifetime<'_> {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("release"), []);
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.0.join("broker-started").exists()
            && !self.0.join("service-done").exists()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn filename(root: &Path) -> PathBuf {
    root.join(std::ffi::OsString::from_vec(
        b"01-01 song # ' \xff.flac".to_vec(),
    ))
}

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    let root = fixture.path();
    fs::create_dir(root.join("services")).unwrap();
    fs::create_dir(root.join("private-profile")).unwrap();
    let recorder = root.join("scoped-player");
    fs::write(&recorder, r#"#!/bin/sh
printf '%s\0' "$@" > "$HEARTHSPACE_TEST_FILE_ACTION_ROOT/arguments"
printf '%s\n' "$DBUS_SESSION_BUS_ADDRESS" "$XDG_CONFIG_HOME" "$WAYLAND_DISPLAY" "$DISPLAY" "$HEARTHSPACE_TEST_SCOPE" > "$HEARTHSPACE_TEST_FILE_ACTION_ROOT/environment"
printf '%s\n' "$DRI_PRIME" "$__NV_PRIME_RENDER_OFFLOAD" > "$HEARTHSPACE_TEST_FILE_ACTION_ROOT/gpu-environment"
touch "$HEARTHSPACE_TEST_FILE_ACTION_ROOT/ready"
"#).unwrap();
    fs::set_permissions(&recorder, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.join(format!("share/applications/{APP}.desktop")), format!(
        "[Desktop Entry]\nType=Application\nName=Cold Player\nDBusActivatable=true\nExec=\"{}\" %F %k %c\n", recorder.display())).unwrap();
    let service_command = format!(
        "/usr/bin/env {} {} --exact {SERVICE} --nocapture",
        gio::glib::shell_quote(format!("{CHILD_ROOT}={}", root.display()))
            .into_string()
            .unwrap(),
        gio::glib::shell_quote(std::env::current_exe().unwrap())
            .into_string()
            .unwrap()
    );
    fs::write(
        root.join(format!("services/{APP}.service")),
        format!("[D-BUS Service]\nName={APP}\nExec={service_command}\n"),
    )
    .unwrap();
    let helper = root.join("helper");
    fs::write(
        &helper,
        format!(
            r#"#!/bin/sh
export HEARTHSPACE_TEST_HELPER_MODE="$1"
export HEARTHSPACE_TEST_HELPER_DESKTOP="$2"
export HEARTHSPACE_TEST_HELPER_FILE="$3"
exec "$HEARTHSPACE_TEST_EXECUTABLE" --exact {HELPER} --nocapture
"#
        ),
    )
    .unwrap();
    fs::set_permissions(helper, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(filename(root), []).unwrap();
    fixture
}

#[test]
fn cold_native_handler_uses_private_dbus_activation() {
    let fixture = fixture();
    let bus = Bus::start_with_services(fixture.path());
    let _service = ServiceLifetime(fixture.path());
    assert_success(
        fixture
            .child(LAUNCH)
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
            .env("HEARTHSPACE_TEST_LAUNCH_MODE", "native"),
    );
    // Successful helper completion means Open was acknowledged, not merely queued.
    // Keep this assertion immediate: waiting longer would hide premature success.
    let uris: Vec<String> =
        serde_json::from_slice(&fs::read(fixture.path().join("received.json")).unwrap()).unwrap();
    assert_eq!(uris, [file_uri(&filename(fixture.path())).unwrap()]);
    assert!(fixture.path().join("broker-started").exists());
    assert!(
        !fixture.path().join("ready").exists(),
        "native handler must not fall back to Exec"
    );
    // The same supervised helper must deliver to an owner that is already up.
    fs::remove_file(fixture.path().join("received.json")).unwrap();
    assert_success(
        fixture
            .child(LAUNCH)
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
            .env("HEARTHSPACE_TEST_LAUNCH_MODE", "native"),
    );
    let warm_uris: Vec<String> =
        serde_json::from_slice(&fs::read(fixture.path().join("received.json")).unwrap()).unwrap();
    assert_eq!(warm_uris, uris);
    assert!(!fixture.path().join("ready").exists());
    fs::write(fixture.path().join("release"), []).unwrap();
    wait_for(&fixture.path().join("service-done"));
}

#[test]
fn cold_native_handler_reports_rejected_open() {
    let fixture = fixture();
    fs::write(fixture.path().join("reject-open"), []).unwrap();
    let bus = Bus::start_with_services(fixture.path());
    let _service = ServiceLifetime(fixture.path());
    assert_success(
        fixture
            .child(LAUNCH)
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
            .env("HEARTHSPACE_TEST_LAUNCH_MODE", "native-rejected"),
    );
    assert!(fixture.path().join("received.json").exists());
    assert!(fixture.path().join("broker-started").exists());
    assert!(!fixture.path().join("ready").exists());
    fs::write(fixture.path().join("release"), []).unwrap();
    wait_for(&fixture.path().join("service-done"));
}

#[test]
fn cold_scoped_handler_inherits_scope_and_profile_without_broker_activation() {
    let fixture = fixture();
    let bus = Bus::start_with_services(fixture.path());
    assert_success(
        fixture
            .child(LAUNCH)
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
            .env("HEARTHSPACE_TEST_LAUNCH_MODE", "scoped"),
    );
    wait_for(&fixture.path().join("ready"));
    assert!(!fixture.path().join("broker-started").exists());
    let desktop = fixture
        .path()
        .join(format!("share/applications/{APP}.desktop"));
    let file = filename(fixture.path());
    let args = fs::read(fixture.path().join("arguments")).unwrap();
    assert_eq!(
        args.strip_suffix(&[0])
            .unwrap()
            .split(|byte| *byte == 0)
            .collect::<Vec<_>>(),
        [
            file.as_os_str().as_encoded_bytes(),
            desktop.as_os_str().as_encoded_bytes(),
            b"Cold Player".as_slice()
        ]
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("environment")).unwrap(),
        format!(
            "{}\n{}\nwayland-scoped\n:91\nowned\n",
            bus.address,
            fixture.path().join("private-profile").display()
        )
    );
}

#[test]
fn launch_in_isolated_environment() {
    let Some(root) = child_root() else { return };
    let app = installed_application(&format!("{APP}.desktop")).unwrap();
    assert!(app.dbus_activatable);
    let mode = std::env::var("HEARTHSPACE_TEST_LAUNCH_MODE").unwrap();
    let paths = if mode.starts_with("activate") {
        Vec::new()
    } else {
        vec![filename(&root)]
    };
    let gpu_environment = if mode == "scoped-gpu" {
        assert!(app.prefers_non_default_gpu);
        crate::gpu::launch_environment(app.prefers_non_default_gpu)
    } else {
        Vec::new()
    };
    let mut command = if mode.starts_with("scoped") {
        let profile = root.join("private-profile");
        let mut environment = vec![
            ("DISPLAY", ":91"),
            ("XDG_CONFIG_HOME", profile.to_str().unwrap()),
        ];
        environment.extend(
            gpu_environment
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
        scoped_gio_command(
            &app.desktop_file,
            &paths,
            &["/usr/bin/env".into(), "HEARTHSPACE_TEST_SCOPE=owned".into()],
            "wayland-scoped",
            &environment,
            &root.join("helper"),
        )
        .unwrap()
    } else {
        native_gio_command(
            &app.desktop_file,
            &paths,
            "wayland-native",
            &[],
            &root.join("helper"),
        )
        .unwrap()
    };
    command.env(
        "HEARTHSPACE_TEST_EXECUTABLE",
        std::env::current_exe().unwrap(),
    );
    let result = run_opener(command);
    if mode.ends_with("rejected") {
        assert!(result.unwrap_err().contains("application launcher failed"));
    } else {
        result.unwrap();
    }
}

#[test]
fn gio_helper_in_isolated_environment() -> Result<(), String> {
    if child_root().is_none() {
        return Ok(());
    }
    let arguments = [
        std::env::var_os("HEARTHSPACE_TEST_HELPER_DESKTOP").unwrap(),
        std::env::var_os("HEARTHSPACE_TEST_HELPER_FILE").unwrap(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty());
    match std::env::var("HEARTHSPACE_TEST_HELPER_MODE")
        .unwrap()
        .as_str()
    {
        SCOPED_GIO_FLAG => run_scoped_gio_helper(arguments),
        NATIVE_GIO_FLAG => run_native_gio_helper(arguments),
        mode => panic!("Unexpected helper mode: {mode}"),
    }
}

struct Player(PathBuf);
#[zbus::interface(name = "org.freedesktop.Application")]
impl Player {
    fn activate(
        &self,
        _platform_data: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        let path = self.0.join("activations");
        let count = fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
            .unwrap_or(0);
        fs::write(path, (count + 1).to_string()).unwrap();
        if self.0.join("reject-open").exists() {
            return Err(zbus::fdo::Error::Failed(
                "Cannot activate this application".into(),
            ));
        }
        Ok(())
    }

    fn open(
        &self,
        uris: Vec<String>,
        _platform_data: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        fs::write(
            self.0.join("received.json"),
            serde_json::to_vec(&uris).unwrap(),
        )
        .unwrap();
        if self.0.join("reject-open").exists() {
            return Err(zbus::fdo::Error::Failed("Cannot open this file".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn serve_cold_player() {
    let Some(root) = child_root() else { return };
    let address = std::env::var("DBUS_STARTER_ADDRESS").unwrap();
    fs::write(root.join("broker-started"), []).unwrap();
    let player = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name(APP)
        .unwrap()
        .serve_at("/org/hearthspace/ColdPlayer", Player(root.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("release").exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(player);
    fs::write(root.join("service-done"), []).unwrap();
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists(), "missing {}", path.display());
}
