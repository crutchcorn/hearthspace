use super::*;

#[test]
fn shell_pointer_zoom_reveals_distant_app_without_stealing_shell_focus() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let (_fixture, state) = maximized_fixture(&compositor, "distant-app", "wayland");
    let first = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["maximized"] == true
    })[0]
        .clone();
    let id = first["id"].as_u64().unwrap();
    action(&compositor, format!("restore-window {id}"));
    wait_state(&state, |state| {
        state["maximized"] == false && state["width"] == 560
    });
    action(&compositor, format!("move-window {id} 7000 -3000"));
    action(&compositor, format!("maximize-window {id} true"));
    wait_state(&state, |state| state["maximized"] == true);

    let bar_state = compositor.runtime_dir.path().join("shell-bar.json");
    let log = std::fs::File::create(bar_state.with_extension("log")).unwrap();
    let _bar = Editors(vec![
        Command::new(env!("CARGO_BIN_EXE_hearthspace-window-test"))
            .arg(&bar_state)
            .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .env("GDK_BACKEND", "wayland")
            .env("HEARTHSPACE_WINDOW_TEST_SHELL_BAR", "1")
            .env(
                "HEARTHSPACE_COMMAND_SOCKET",
                &compositor.command_socket_path,
            )
            .env_remove("DISPLAY")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    ]);
    wait_state(&bar_state, |state| state["mapped"] == true);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = query(&compositor, "status");
        if status["shell_ready"] == true {
            std::fs::write(
                compositor.runtime_dir.path().join("shell-before.json"),
                serde_json::to_vec(&status).unwrap(),
            )
            .unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shell buffer did not map: {status}"
        );
        thread::sleep(Duration::from_millis(25));
    }
    std::fs::write(
        compositor.runtime_dir.path().join("shell-before.png"),
        compositor.take_screenshot(),
    )
    .unwrap();
    // GTK can report mapped and positive bounds while the first allocation's
    // transform is still centered at the origin. Observe valid client geometry
    // only after the compositor has received and rendered its first buffer.
    let bar = wait_state(&bar_state, |state| {
        let button = &state["button"];
        button["x"].as_f64().is_some_and(|x| x >= 0.0)
            && button["y"].as_f64().is_some_and(|y| y >= 0.0)
            && button["width"].as_f64().is_some_and(|width| width > 0.0)
            && button["height"].as_f64().is_some_and(|height| height > 0.0)
    });
    std::fs::write(
        compositor.runtime_dir.path().join("shell-button.json"),
        serde_json::to_vec(&bar).unwrap(),
    )
    .unwrap();
    let button = &bar["button"];
    let x = button["x"].as_f64().unwrap() + button["width"].as_f64().unwrap() / 2.0;
    let y = button["y"].as_f64().unwrap() + button["height"].as_f64().unwrap() / 2.0;
    assert!(
        x > 0.0 && x < 1280.0 && y > 0.0 && y < 48.0,
        "button center outside bar: {bar}"
    );
    // This actual button click transfers keyboard focus to a mapped ShellBar
    // surface before that client's callback sends the zoom command.
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    wait_state(&bar_state, |state| {
        state["clicked"] == true && state["active"] == true
    });
    wait_scale(&compositor, 0.8);
    wait_state(&state, |state| {
        state["maximized"] == false && state["active"] == false
    });
    let window = wait_windows(&compositor, |windows| {
        windows[0]["maximized"] == false
            && windows[0]["visible"] == true
            && windows[0]["focused"] == false
    })[0]
        .clone();
    assert_eq!(window["canvas_x"], 7000);
    assert_eq!(window["canvas_y"], -3000);
    assert!(
        green_pixels(&compositor) > 50_000,
        "restored app was not rendered in the viewport"
    );
    assert_eq!(
        wait_state(&bar_state, |state| state["clicked"] == true)["active"],
        true
    );
    action(&compositor, format!("close-window {id}"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
