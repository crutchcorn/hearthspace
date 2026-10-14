//! Saved spatial preferences survive a graceful compositor restart.
use super::navigation::{Editors, action, query, wait_windows};
use super::*;

fn client(compositor: &HeadlessCompositor) -> Editors {
    let log = std::fs::File::create(compositor.runtime_dir.path().join("client.log")).unwrap();
    Editors(vec![
        Command::new(env!("CARGO_BIN_EXE_hearthspace-window-test"))
            .arg(compositor.runtime_dir.path().join("state.json"))
            .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .env("GDK_BACKEND", "wayland")
            .env("GSK_RENDERER", "cairo")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    ])
}

#[test]
fn layout_restart_restores_canvas_and_positions_without_relaunching_clients() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let viewport;
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let _client = client(&compositor);
        let windows = wait_windows(&compositor, |windows| windows.len() == 1);
        let id = windows[0]["id"].as_u64().unwrap();
        action(&compositor, format!("move-window {id} 2400 -1100"));
        action(&compositor, format!("focus-window {id}"));
        action(&compositor, "zoom-out");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = query(&compositor, "status");
            if state["viewport"]["scale"].as_f64().unwrap() == 0.8 {
                viewport = state["viewport"].clone();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "viewport animation did not complete"
            );
            thread::sleep(Duration::from_millis(25));
        }
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let _client = client(&compositor);
    let windows = wait_windows(&compositor, |windows| windows.len() == 1);
    assert_eq!(windows[0]["canvas_x"], 2400);
    assert_eq!(windows[0]["canvas_y"], -1100);
    assert_eq!(windows[0]["focused"], true);
    assert_eq!(windows[0]["visible"], true);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
