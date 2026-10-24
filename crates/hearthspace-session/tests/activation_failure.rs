//! Exercise failed shared-bus publication through the real session supervisor.
//! Only the broker, manager, and ready desktop children are fixture substitutes.
use std::{collections::BTreeMap, fs, time::Instant};

#[path = "activation_failure/fixture.rs"]
mod fixture;
#[path = "activation_failure/manager.rs"]
mod manager;

use fixture::Fixture;
use manager::MARKER;

#[test]
fn failed_publication_and_rollback_retain_receipt_after_stopping_owned_children() {
    let started = Instant::now();
    let mut fixture = Fixture::new();
    let supervisor = fixture.start_supervisor();
    let status = fixture.wait(supervisor);
    assert!(!status.success(), "{}", fixture.log(supervisor));
    assert!(
        fixture
            .log(supervisor)
            .contains("activation user-manager operation failed"),
        "{}",
        fixture.log(supervisor)
    );

    // The real updater treats SetEnvironment failure as nonfatal. Verification
    // then fails reading the manager, as does the local guard's Drop rollback.
    let (publication, marker) = {
        let state = fixture.state.lock().unwrap();
        assert_eq!(&state.events[..3], &["read", "replace", "set-failed"]);
        assert!(
            state.events[3..]
                .iter()
                .filter(|event| **event == "read-failed")
                .count()
                >= 2,
            "both publication verification and local rollback must fail"
        );
        assert_eq!(state.set_calls.len(), 1);
        assert_eq!(state.replacements.len(), 1);
        let marker = state.values[MARKER].clone();
        assert!(!marker.is_empty());
        assert_eq!(
            state.replacements[0],
            BTreeMap::from([(MARKER.into(), marker.clone())])
        );
        (state.set_calls[0].clone(), marker)
    };
    fixture.assert_children_stopped();
    fixture.assert_broker_alive();
    fixture.assert_profile_untouched();

    // Old Supervisor saw activation_environment == None and deleted this
    // receipt despite the local guard's failed rollback. Keep this assertion
    // before checking the additional shutdown retry so that regression is clear.
    let receipt = fixture.runtime.join("activation-environment.json");
    assert!(
        receipt.is_file(),
        "failed publication lost its recovery receipt: {}",
        fixture.log(supervisor)
    );
    assert!(fixture.runtime.is_dir());
    let record: serde_json::Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    assert_eq!(record["bus"], fixture.address());
    assert_eq!(record["session_dir"], fixture.runtime.to_str().unwrap());
    assert_eq!(record["host_runtime"], fixture.host.to_str().unwrap());
    assert_eq!(
        record["published"],
        serde_json::to_value(&publication).unwrap()
    );
    assert_eq!(
        record["previous"],
        serde_json::to_value(&fixture.previous).unwrap()
    );
    assert!(
        fixture.state.lock().unwrap().events[3..]
            .iter()
            .filter(|event| **event == "read-failed")
            .count()
            >= 3,
        "shutdown must retry retirement even though publication returned no guard"
    );
    let processes: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.runtime.join("processes.json")).unwrap()).unwrap();
    for role in ["settingsd", "compositor"] {
        assert_eq!(processes[role], fixture.child_identity(role)["pid"]);
    }
    assert_eq!(processes["shell"], 0);
    assert!(!fixture.runtime.join("app-scope-owner").exists());

    // A fresh D-Bus activation sees the partial publication, although the
    // synthetic manager still has its previous values plus the claimed marker.
    let activated = fixture.probe(0);
    assert_eq!(
        activated["WAYLAND_DISPLAY"].as_deref(),
        Some(publication["WAYLAND_DISPLAY"].as_str())
    );
    assert_eq!(
        activated["HEARTHSPACE_SESSION_DIR"].as_deref(),
        fixture.runtime.to_str()
    );
    assert_eq!(
        activated["XDG_RUNTIME_DIR"].as_deref(),
        fixture.host.to_str()
    );
    assert_eq!(activated[MARKER].as_deref(), Some(marker.as_str()));
    assert_eq!(
        activated["PATH"].as_deref(),
        Some(publication["PATH"].as_str())
    );

    // Once the external manager recovers, the actual standalone guard can
    // replay the retained receipt without a live Supervisor or owned children.
    fixture.state.lock().unwrap().fail_reads = false;
    let guard = fixture.start_retirement();
    assert!(fixture.wait(guard).success(), "{}", fixture.log(guard));
    assert!(!receipt.exists());
    assert!(
        fixture.runtime.is_dir(),
        "the fixture owns final directory removal"
    );
    assert_eq!(fixture.state.lock().unwrap().values, fixture.previous);
    let restored = fixture.probe(1);
    for key in ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "PATH"] {
        assert_eq!(
            restored[key].as_deref(),
            Some(fixture.previous[key].as_str())
        );
    }
    assert_eq!(restored["HEARTHSPACE_SESSION_DIR"].as_deref(), Some(""));
    assert_eq!(restored[MARKER].as_deref(), Some(""));
    fixture.assert_profile_untouched();
    fixture.assert_broker_alive();
    drop(fixture);
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}
