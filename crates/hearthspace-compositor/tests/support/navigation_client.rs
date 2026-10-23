//! Real keyboard client fixtures shared by navigation and window overlay tests.
use super::navigation::{Editors, wait_windows};
use super::*;
use serde_json::Value;

pub(super) fn editor(
    compositor: &HeadlessCompositor,
    editors: &mut Editors,
    number: usize,
) -> (u64, PathBuf) {
    let title = format!("Navigation editor {number}");
    let input = compositor
        .runtime_dir
        .path()
        .join(format!("navigation-{number}.txt"));
    let state = input.with_extension("json");
    std::fs::write(&input, "").unwrap();
    let log = std::fs::File::create(input.with_extension("log")).unwrap();
    editors.0.push(
        Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
            .args([
                "--title",
                &title,
                "--input-file",
                input.to_str().unwrap(),
                "--output-file",
                state.to_str().unwrap(),
            ])
            .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .env("GDK_BACKEND", "wayland")
            .env_remove("DISPLAY")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["title"] == title && window["mapped"] == true && window["focused"] == true
        })
    });
    let id = windows
        .iter()
        .find(|window| window["title"] == title)
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    wait_state(&state, |value| {
        value["ready"] == true && value["active"] == true
    });
    (id, state)
}

pub(super) fn wait_state(path: &Path, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        if condition(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "editor state timed out: {state}");
        thread::sleep(Duration::from_millis(20));
    }
}
