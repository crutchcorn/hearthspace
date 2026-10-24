//! Exercise complementary audio routing against a real activation broker.
use super::*;

#[test]
fn pulse_runtime_override_replaces_stale_server_for_direct_and_cold_apps() {
    roundtrip(
        &[("PULSE_RUNTIME_PATH", "/synthetic/custom-pulse")],
        &[
            ("PULSE_SERVER", "unix:/old/pulse/native"),
            ("PULSE_RUNTIME_PATH", "/old/pulse"),
        ],
        &[
            ("PULSE_SERVER", "unix:/synthetic/custom-pulse/native"),
            ("PULSE_RUNTIME_PATH", "/synthetic/custom-pulse"),
        ],
    );
}

#[test]
fn pipewire_runtime_override_replaces_stale_absolute_remote_for_direct_and_cold_apps() {
    roundtrip(
        &[("PIPEWIRE_RUNTIME_DIR", "/synthetic/custom-pipewire")],
        &[
            ("PIPEWIRE_REMOTE", "/old/pipewire-0"),
            ("PIPEWIRE_RUNTIME_DIR", "/old"),
            ("PIPEWIRE_CORE", "old-core"),
        ],
        &[
            ("PIPEWIRE_REMOTE", "pipewire-0"),
            ("PIPEWIRE_RUNTIME_DIR", "/synthetic/custom-pipewire"),
            ("PIPEWIRE_CORE", "pipewire-0"),
        ],
    );
}

#[test]
fn explicit_empty_audio_servers_and_custom_core_survive_publication_and_retirement() {
    let explicit = &[
        ("PULSE_SERVER", ""),
        ("PIPEWIRE_REMOTE", ""),
        ("PIPEWIRE_CORE", "custom-core"),
    ];
    roundtrip(
        explicit,
        &[
            ("PULSE_SERVER", "unix:/old/pulse/native"),
            ("PIPEWIRE_REMOTE", "/old/pipewire-0"),
            ("PIPEWIRE_CORE", "old-core"),
        ],
        explicit,
    );
}

fn roundtrip(overrides: &[(&str, &str)], baseline: &[(&str, &str)], expected: &[(&str, &str)]) {
    let bus = Bus::start();
    let paths = bus.paths("audio-routing");
    let baseline: BTreeMap<_, _> = pairs(baseline).into_iter().collect();
    bus.seed(baseline.clone());
    let mut session = bus.session(&paths);
    session.vars.extend(pairs(overrides));
    let mut guard = update_activation_environment(
        &paths,
        &session,
        false,
        Instant::now() + Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap()
    .unwrap();
    // This is the direct child environment construction used by session_env.
    let explicit: BTreeMap<_, _> = pairs(overrides).into_iter().collect();
    let direct_env = crate::audio_env::resolved(paths.host_runtime_dir.to_str().unwrap(), |key| {
        explicit.get(key).cloned()
    });
    let output = std::process::Command::new("/usr/bin/python3")
        .args(["-I", "-S", "-c", "import os,json; print(json.dumps({k:os.environ.get(k) for k in ['PULSE_SERVER','PULSE_RUNTIME_PATH','PIPEWIRE_RUNTIME_DIR','PIPEWIRE_REMOTE','PIPEWIRE_CORE']}))"])
        .env_clear().envs(direct_env).output().unwrap();
    assert!(output.status.success());
    let direct: BTreeMap<String, Option<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let cold = bus.probe();
    let manager = bus.values();
    for (key, value) in &direct {
        assert_eq!(value, &cold[key], "direct/cold differs for {key}");
        assert_eq!(
            value.as_ref(),
            manager.get(key),
            "direct/manager differs for {key}"
        );
    }
    for &(key, value) in expected {
        assert_eq!(
            direct[key].as_deref(),
            Some(value),
            "wrong routing for {key}"
        );
    }
    let receipt = cleanup::read_record(&paths.session_dir).unwrap().unwrap();
    for (key, value) in &baseline {
        assert_eq!(&receipt.previous[key], value);
    }
    guard.retire().unwrap();
    let cold = bus.probe();
    for (key, value) in &baseline {
        assert_eq!(&bus.values()[key], value);
        assert_eq!(cold[key].as_ref(), Some(value));
    }
}

#[test]
fn retirement_with_absent_audio_baseline_restores_usable_broker_defaults() {
    let bus = Bus::start();
    let paths = bus.paths("absent-audio");
    let mut session = bus.session(&paths);
    session.vars.extend(pairs(&[
        ("PULSE_RUNTIME_PATH", "/synthetic/session-pulse"),
        ("PIPEWIRE_RUNTIME_DIR", "/synthetic/session-pipewire"),
    ]));
    let mut guard = update_activation_environment(
        &paths,
        &session,
        false,
        Instant::now() + Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap()
    .unwrap();
    guard.retire().unwrap();
    let cold = bus.probe();
    for key in [
        "PULSE_SERVER",
        "PULSE_RUNTIME_PATH",
        "PIPEWIRE_RUNTIME_DIR",
        "PIPEWIRE_REMOTE",
        "PIPEWIRE_CORE",
    ] {
        assert!(
            !bus.values().contains_key(key),
            "absent manager baseline must remain absent: {key}"
        );
    }
    assert_eq!(
        cold["PULSE_SERVER"].as_deref(),
        Some(format!("unix:{}/pulse/native", paths.host_runtime_dir.display()).as_str())
    );
    assert_eq!(
        cold["PULSE_RUNTIME_PATH"].as_deref(),
        paths.host_runtime_dir.join("pulse").to_str()
    );
    assert_eq!(
        cold["PIPEWIRE_RUNTIME_DIR"].as_deref(),
        paths.host_runtime_dir.to_str()
    );
    assert_eq!(cold["PIPEWIRE_REMOTE"].as_deref(), Some("pipewire-0"));
    assert_eq!(cold["PIPEWIRE_CORE"].as_deref(), Some("pipewire-0"));
}

#[test]
fn retirement_preserves_independently_changed_audio_runtime_and_resolves_its_complement() {
    let bus = Bus::start();
    let paths = bus.paths("changed-audio");
    let mut guard = bus.publish(&paths);
    bus.state
        .lock()
        .unwrap()
        .values
        .insert("PIPEWIRE_RUNTIME_DIR".into(), "/synthetic/new-owner".into());
    guard.retire().unwrap();
    assert_eq!(bus.values()["PIPEWIRE_RUNTIME_DIR"], "/synthetic/new-owner");
    assert!(!bus.values().contains_key("PIPEWIRE_REMOTE"));
    let cold = bus.probe();
    assert_eq!(
        cold["PIPEWIRE_RUNTIME_DIR"].as_deref(),
        Some("/synthetic/new-owner")
    );
    assert_eq!(cold["PIPEWIRE_REMOTE"].as_deref(), Some("pipewire-0"));
}
