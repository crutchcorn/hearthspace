//! Real broker + manager boundary: live updates, startup ordering and retirement.
use super::*;
use cleanup::{DisplayUpdate, MARKER};

const GENERATION: &str = "01234567-89ab-cdef-0123-456789abcdef";

fn initialize(bus: &Bus, paths: &SessionPaths, private: bool) -> SessionEnv {
    cleanup::initialize_display(
        &paths.host_runtime_dir,
        &paths.session_dir,
        &bus.address,
        private,
        GENERATION,
    )
    .unwrap();
    let mut session = bus.session(paths);
    session.vars.push((MARKER.into(), GENERATION.into()));
    session
}

fn publish(bus: &Bus, paths: &SessionPaths, session: &SessionEnv) -> ActivationEnvironment {
    let guard = update_activation_environment(
        paths,
        session,
        false,
        Instant::now() + Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap()
    .unwrap();
    assert_eq!(bus.values()[MARKER], GENERATION);
    guard
}

fn update(paths: &SessionPaths, revision: u64, display: Option<&str>) -> io::Result<()> {
    DisplayUpdate {
        session: paths.session_dir.clone(),
        generation: GENERATION.into(),
        revision,
        display: display.map(str::to_owned),
    }
    .publish(
        Instant::now() + Duration::from_secs(2),
        &AtomicBool::new(false),
    )
}

#[test]
fn live_display_reaches_cold_activation_and_original_guard_restores_the_baseline() {
    let bus = Bus::start();
    let paths = bus.paths("live-display");
    bus.seed(pairs(&[("DISPLAY", ":91")]).into_iter().collect());
    let session = initialize(&bus, &paths, false);
    let mut guard = publish(&bus, &paths, &session);
    update(&paths, 1, Some(":5")).unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":5");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":5"));
    let receipt = cleanup::read_record(&paths.session_dir).unwrap().unwrap();
    assert_eq!(receipt.previous["DISPLAY"], ":91");
    assert_eq!(receipt.published["DISPLAY"], ":5");
    assert!(receipt.pending_display.is_none());
    // Guard predates X11; it must reload the latest same-generation journal.
    guard.retire().unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":91");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":91"));
}

#[test]
fn x11_ready_before_publication_and_disconnect_use_the_latest_revision() {
    let bus = Bus::start();
    let paths = bus.paths("early-x11");
    bus.seed(pairs(&[("DISPLAY", ":92")]).into_iter().collect());
    let session = initialize(&bus, &paths, false);
    update(&paths, 1, Some(":6")).unwrap();
    assert_eq!(
        bus.values()["DISPLAY"],
        ":92",
        "unowned startup cannot publish"
    );
    let mut guard = publish(&bus, &paths, &session);
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":6"));
    update(&paths, 2, None).unwrap();
    assert!(update(&paths, 1, Some(":6")).is_err());
    assert_eq!(bus.values()["DISPLAY"], "");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(""));
    guard.retire().unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":92");
}

#[test]
fn stale_generation_and_external_display_cannot_be_overwritten() {
    let bus = Bus::start();
    let paths = bus.paths("ownership");
    let session = initialize(&bus, &paths, false);
    let mut guard = publish(&bus, &paths, &session);
    let stale = DisplayUpdate {
        session: paths.session_dir.clone(),
        generation: "older-generation".into(),
        revision: 100,
        display: Some(":9".into()),
    };
    assert!(
        stale
            .publish(
                Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(false)
            )
            .is_err()
    );
    bus.state
        .lock()
        .unwrap()
        .values
        .insert("DISPLAY".into(), ":99".into());
    assert!(update(&paths, 1, Some(":7")).is_err());
    assert_eq!(bus.values()["DISPLAY"], ":99");
    guard.retire().unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":99");
}

#[test]
fn partial_live_update_journal_recovers_both_targets_without_losing_baseline() {
    let bus = Bus::start();
    let paths = bus.paths("partial-display");
    bus.seed(pairs(&[("DISPLAY", ":93")]).into_iter().collect());
    let session = initialize(&bus, &paths, false);
    let mut guard = publish(&bus, &paths, &session);
    bus.state.lock().unwrap().fail_replace_after = Some(1); // Initial claim succeeded.
    assert!(update(&paths, 1, Some(":8")).is_err());
    let record = cleanup::read_record(&paths.session_dir).unwrap().unwrap();
    assert_eq!(record.previous["DISPLAY"], ":93");
    assert_eq!(record.pending_display.as_deref(), Some(""));
    assert_eq!(record.published["DISPLAY"], ":8");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":8"));
    assert_eq!(bus.values()["DISPLAY"], "");
    bus.state.lock().unwrap().fail_replace_after = None;
    guard.retire().unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":93");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":93"));
}

#[test]
fn private_display_publication_never_reads_or_writes_the_user_manager() {
    let bus = Bus::start();
    let paths = bus.paths("private-display");
    initialize(&bus, &paths, true);
    {
        let mut state = bus.state.lock().unwrap();
        state.stall_reads = true;
        state.values.insert("DISPLAY".into(), ":94".into());
    }
    update(&paths, 1, Some(":9")).unwrap();
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":9"));
    let state = bus.state.lock().unwrap();
    assert_eq!(state.values["DISPLAY"], ":94");
    assert_eq!(state.reads, 0);
    assert!(state.replace_calls.is_empty());
}

#[test]
fn direct_and_cold_activation_share_connection_and_explicit_audio_overrides() {
    let bus = Bus::start();
    let paths = bus.paths("connections");
    let mut session = initialize(&bus, &paths, false);
    session.vars.extend(pairs(&[
        ("PULSE_SERVER", ""),
        ("PIPEWIRE_REMOTE", "custom-pipewire"),
        ("PIPEWIRE_RUNTIME_DIR", "/synthetic/audio"),
        ("PULSE_RUNTIME_PATH", "/synthetic/pulse"),
    ]));
    let mut guard = publish(&bus, &paths, &session);
    update(&paths, 1, Some(":10")).unwrap();
    let (runtime, display) = hearthspace_desktop::environment::connection(
        &paths.host_runtime_dir,
        &paths.session_dir,
        &session.wayland_display,
        false,
    )
    .unwrap();
    let mut direct = hearthspace_desktop::app_catalog::launch_command(&["/usr/bin/python3".into(), "-I".into(), "-S".into(), "-c".into(),
        "import json,os; print(json.dumps({k:os.environ.get(k) for k in ['XDG_RUNTIME_DIR','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','PULSE_SERVER','PULSE_RUNTIME_PATH','PIPEWIRE_RUNTIME_DIR','PIPEWIRE_REMOTE','HEARTHSPACE_SESSION_DIR']}))".into()], &display, &[], None).unwrap();
    direct
        .env_clear()
        .envs(session.vars)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("WAYLAND_DISPLAY", display)
        .env("DISPLAY", ":10");
    let output = direct.output().unwrap();
    assert!(output.status.success());
    let values: BTreeMap<String, Option<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let activated = bus.probe();
    for (key, value) in values {
        assert_eq!(value, activated[&key], "connection differs for {key}");
    }
    guard.retire().unwrap();
}

#[test]
fn cancelled_display_update_cannot_publish_and_reused_session_directory_is_rejected() {
    let bus = Bus::start();
    let paths = bus.paths("reused-display");
    initialize(&bus, &paths, false);
    let ticket = DisplayUpdate {
        session: paths.session_dir.clone(),
        generation: GENERATION.into(),
        revision: 1,
        display: Some(":11".into()),
    };
    assert!(
        ticket
            .publish(
                Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(true)
            )
            .is_err()
    );
    assert_eq!(
        cleanup::latest_display(&paths.session_dir, GENERATION)
            .unwrap()
            .as_deref(),
        Some("")
    );
    let retired = paths.session_dir.with_extension("retired");
    std::fs::rename(&paths.session_dir, &retired).unwrap();
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&paths.session_dir)
            .unwrap();
    }
    std::fs::copy(
        retired.join(cleanup::DISPLAY_RECORD),
        paths.session_dir.join(cleanup::DISPLAY_RECORD),
    )
    .unwrap();
    assert!(
        ticket
            .publish(
                Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(false)
            )
            .is_err()
    );
}

#[test]
fn an_interrupted_final_journal_write_does_not_poison_the_next_update_baseline() {
    let bus = Bus::start();
    let paths = bus.paths("interrupted-journal");
    bus.seed(pairs(&[("DISPLAY", ":95")]).into_iter().collect());
    let session = initialize(&bus, &paths, false);
    let mut guard = publish(&bus, &paths, &session);
    update(&paths, 1, Some(":12")).unwrap();
    // Both targets accepted :12, but the crash left the prior journal's pending value.
    let mut record = cleanup::read_record(&paths.session_dir).unwrap().unwrap();
    record.pending_display = Some(String::new());
    std::fs::write(
        paths.session_dir.join(cleanup::RECORD),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    bus.state.lock().unwrap().fail_replace_after = Some(2);
    assert!(update(&paths, 2, Some(":13")).is_err());
    assert_eq!(
        cleanup::read_record(&paths.session_dir)
            .unwrap()
            .unwrap()
            .pending_display
            .as_deref(),
        Some(":12")
    );
    bus.state.lock().unwrap().fail_replace_after = None;
    guard.retire().unwrap();
    assert_eq!(bus.values()["DISPLAY"], ":95");
    assert_eq!(bus.probe()["DISPLAY"].as_deref(), Some(":95"));
}

#[test]
fn independently_removed_display_is_not_claimed_by_a_live_update() {
    let bus = Bus::start();
    let paths = bus.paths("removed-display");
    let session = initialize(&bus, &paths, false);
    let mut guard = publish(&bus, &paths, &session);
    bus.state.lock().unwrap().values.remove("DISPLAY");
    assert!(update(&paths, 1, Some(":14")).is_err());
    assert!(!bus.values().contains_key("DISPLAY"));
    guard.retire().unwrap();
    assert!(!bus.values().contains_key("DISPLAY"));
}
