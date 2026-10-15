use super::*;
use std::{
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "hearthspace-gtk-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn python_available(module: &str) -> bool {
    crate::bounded_command::output(
        Command::new("/usr/bin/python3").args(["-c", module]),
        Instant::now() + Duration::from_secs(3),
        &AtomicBool::new(false),
    )
    .is_ok_and(|result| result.status.success())
}

fn query(root: &Path, code: &str, arguments: &[&str]) -> String {
    let output = crate::bounded_command::output(
        Command::new("/usr/bin/python3")
            .args(["-c", code])
            .args(arguments)
            .env("XDG_CONFIG_HOME", root.join("user"))
            .env(
                "XDG_CONFIG_DIRS",
                merge_config_dirs(root, Some(OsStr::new("/missing"))).unwrap(),
            )
            .env("XDG_DATA_DIRS", root.join("share"))
            .env("XDG_CURRENT_DESKTOP", hearthspace_common::CURRENT_DESKTOP)
            .env("GSETTINGS_BACKEND", "keyfile")
            .env("DISPLAY", "")
            .env("WAYLAND_DISPLAY", "/not-a-display")
            .env_remove("GSETTINGS_SCHEMA_DIR"),
        Instant::now() + Duration::from_secs(3),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn config_search_preserves_existing_directories_and_user_home() {
    assert_eq!(
        merge_config_dirs(
            Path::new("/private/session"),
            Some(OsStr::new("/custom/one:/custom/two"))
        )
        .unwrap(),
        "/private/session/desktop-config:/custom/one:/custom/two"
    );
    for existing in [None, Some(OsStr::new(""))] {
        assert_eq!(
            merge_config_dirs(Path::new("/private/session"), existing).unwrap(),
            "/private/session/desktop-config:/etc/xdg"
        );
    }
}

#[test]
fn actual_gsettings_uses_desktop_default_without_changing_saved_user_value() {
    if which::which("glib-compile-schemas").is_err()
        || !python_available("from gi.repository import Gio")
    {
        eprintln!("skipping optional GLib/PyGObject integration test");
        return;
    }
    let root = Scratch::new();
    let sources = root.0.join("source/glib-2.0/schemas");
    fs::create_dir_all(&sources).unwrap();
    fs::write(sources.join("org.gnome.desktop.wm.preferences.gschema.xml"), r#"<schemalist><schema id="org.gnome.desktop.wm.preferences" path="/org/gnome/desktop/wm/preferences/"><key name="button-layout" type="s"><default>'appmenu:close'</default></key><key name="focus-mode" enum="example.Focus"><default>'click'</default></key></schema></schemalist>"#).unwrap();
    fs::write(sources.join("org.gnome.desktop.enums.xml"), r#"<schemalist><enum id="example.Focus"><value nick="click" value="0"/><value nick="sloppy" value="1"/></enum></schemalist>"#).unwrap();
    fs::write(sources.join("10-existing.gschema.override"), "[org.gnome.desktop.wm.preferences]\nfocus-mode='sloppy'\n[unrelated]\nkey='must not break compilation'\n").unwrap();
    write_defaults(&root.0, root.0.join("source").to_str().unwrap()).unwrap();
    let code = "from gi.repository import Gio\nimport sys\ns=Gio.Settings.new('org.gnome.desktop.wm.preferences')\nif len(sys.argv)>1: s.set_string('button-layout', sys.argv[1]); Gio.Settings.sync()\nprint(s.get_string('button-layout')); print(s.get_string('focus-mode'))";
    assert_eq!(query(&root.0, code, &[]), ":maximize,close\nsloppy");
    assert_eq!(
        query(&root.0, code, &["close:maximize"]),
        "close:maximize\nsloppy"
    );
    // A fresh client sees the saved preference rather than our schema default.
    assert_eq!(query(&root.0, code, &[]), "close:maximize\nsloppy");
}

#[test]
fn actual_gtk_files_keep_user_layout_above_session_default() {
    for version in ["3.0", "4.0"] {
        if !python_available(&format!(
            "import gi; gi.require_version('Gtk', '{version}'); from gi.repository import Gtk"
        )) {
            eprintln!("skipping optional GTK {version} settings test");
            continue;
        }
        let root = Scratch::new();
        let directory = root.0.join("desktop-config").join(format!("gtk-{version}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("settings.ini"), GTK_SETTINGS).unwrap();
        // Instantiating settings parses the real library's files without a
        // display, renderer, or host settings daemon. The native suite checks
        // the final Wayland/GSettings property and actual Firefox controls.
        let code = format!(
            "import gi; gi.require_version('Gtk', '{version}'); from gi.repository import Gtk; print(Gtk.Settings().get_property('gtk-decoration-layout'))"
        );
        assert_eq!(query(&root.0, &code, &[]), ":maximize,close");
        let user = root.0.join("user").join(format!("gtk-{version}"));
        fs::create_dir_all(&user).unwrap();
        fs::write(
            user.join("settings.ini"),
            "[Settings]\ngtk-decoration-layout=close:maximize\n",
        )
        .unwrap();
        assert_eq!(query(&root.0, &code, &[]), "close:maximize");
    }
}

#[test]
fn missing_optional_schema_still_writes_gtk_fallback_files() {
    let root = Scratch::new();
    let error = write_defaults(&root.0, root.0.join("missing").to_str().unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    for version in ["gtk-3.0", "gtk-4.0"] {
        assert_eq!(
            fs::read_to_string(
                root.0
                    .join("desktop-config")
                    .join(version)
                    .join("settings.ini")
            )
            .unwrap(),
            GTK_SETTINGS
        );
    }
}
