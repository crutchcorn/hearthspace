use std::os::unix::ffi::OsStringExt;

use super::*;

const AMD: &str = "/sys/devices/pci0000:00/0000:01:00.0";
const NVIDIA: &str = "/sys/devices/pci0000:00/0000:02:00.0";
const OTHER: &str = "/sys/devices/platform/other-gpu";

fn device(device_id: u64, path: &str) -> UdevDeviceInfo {
    UdevDeviceInfo {
        device_id,
        path: path.into(),
    }
}

fn seat_devices() -> Vec<UdevDeviceInfo> {
    vec![
        device(4, "/dev/dri/card4"),
        device(2, "/dev/dri/card2"),
        device(8, "/dev/dri/card8"),
    ]
}

fn resolve(path: &Path) -> io::Result<String> {
    Ok(match path.to_str().unwrap() {
        "/dev/dri/card4" => AMD,
        "/dev/dri/card2" => OTHER,
        "/dev/dri/card8" => NVIDIA,
        unexpected => panic!("lookup must use an enumerated devnode: {unexpected}"),
    }
    .into())
}

fn preferred(devices: &[UdevDeviceInfo]) -> StartupRendererPreference {
    StartupRendererPreference::from_value(Some(NVIDIA.into()), devices, resolve).unwrap()
}

fn ids(devices: &[UdevDeviceInfo]) -> Vec<u64> {
    devices.iter().map(|device| device.device_id).collect()
}

fn fallback_reason(preference: &StartupRendererPreference) -> &str {
    preference
        .pending
        .as_ref()
        .unwrap()
        .fallback_reason
        .as_deref()
        .unwrap()
}

#[test]
fn unset_preserves_automatic_order_without_resolving_identities() {
    let devices = seat_devices();
    let mut preference = StartupRendererPreference::from_value(None, &devices, |_| {
        panic!("automatic startup must not require physical identity preflight")
    })
    .unwrap();
    let ordered = preference.opening_order(&devices, true, |_| {
        panic!("automatic opening must not add identity lookups")
    });
    assert_eq!(ids(&ordered), [4, 2, 8]);
    assert_eq!(preference.scene_after_open(None, 4, AMD), 4);
    assert_eq!(preference.scene_after_open(Some(4), 8, NVIDIA), 4);
}

#[test]
fn preferred_moves_first_and_keeps_every_other_gpu_in_automatic_order() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    let ordered = preference.opening_order(&devices, true, resolve);
    assert_eq!(ids(&ordered), [8, 4, 2]);
    assert_eq!(ids(&devices), [4, 2, 8]);
    assert_eq!(ordered[0].path, Path::new("/dev/dri/card8"));
    assert_eq!(ordered[1].path, Path::new("/dev/dri/card4"));
    assert_eq!(ordered[2].path, Path::new("/dev/dri/card2"));
    assert_eq!(preference.scene_after_open(None, 8, NVIDIA), 8);
    assert_eq!(preference.scene_after_open(Some(8), 4, AMD), 8);
    assert_eq!(preference.scene_after_open(Some(8), 2, OTHER), 8);
    assert!(preference.pending.is_none());
}

#[test]
fn empty_preference_fails_before_any_identity_lookup() {
    let error = StartupRendererPreference::from_value(Some(OsString::new()), &[], |_| {
        panic!("empty preference must fail before lookup")
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains(ENV_KEY));
    assert!(error.to_string().contains("must not be empty"));
}

#[test]
fn non_utf8_preference_fails_without_lossy_matching() {
    let value = OsString::from_vec(b"/sys/devices/\xff".to_vec());
    let error = StartupRendererPreference::from_value(Some(value), &[], |_| {
        panic!("non-UTF-8 preference must fail before lookup")
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("UTF-8"));
}

#[test]
fn devnode_is_not_accepted_as_a_physical_identity() {
    let error = StartupRendererPreference::from_value(
        Some("/dev/dri/card8".into()),
        &seat_devices(),
        |_| panic!("a requested devnode must never be resolved or opened"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("exact physical DRM identity"));
}

#[test]
fn physical_identity_is_matched_exactly_without_normalizing_user_input() {
    for value in [
        format!("{NVIDIA}/"),
        format!("{NVIDIA} "),
        NVIDIA.replace("/0000:02:00.0", "/./0000:02:00.0"),
        NVIDIA.replace(':', "%3A"),
    ] {
        let error =
            StartupRendererPreference::from_value(Some(value.into()), &seat_devices(), resolve)
                .unwrap_err();
        assert!(error.to_string().contains("does not match"));
    }
}

#[test]
fn off_seat_identity_is_rejected_using_only_enumerated_devnodes() {
    let mut looked_up = Vec::new();
    let error = StartupRendererPreference::from_value(
        Some("/sys/devices/platform/off-seat-gpu".into()),
        &seat_devices(),
        |path| {
            looked_up.push(path.to_path_buf());
            resolve(path)
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match any enumerated seat GPU")
    );
    assert_eq!(
        looked_up,
        seat_devices()
            .into_iter()
            .map(|device| device.path)
            .collect::<Vec<_>>()
    );
}

#[test]
fn explicit_preference_rejects_an_empty_seat() {
    let error = StartupRendererPreference::from_value(Some(NVIDIA.into()), &[], |_| {
        panic!("no enumerated devices to resolve")
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match any enumerated seat GPU")
    );
}

#[test]
fn matching_gpu_does_not_hide_unresolved_identity_later_in_enumeration() {
    let devices = [device(8, "/dev/dri/card8"), device(4, "/dev/dri/card4")];
    let error = StartupRendererPreference::from_value(Some(NVIDIA.into()), &devices, |path| {
        if path == Path::new("/dev/dri/card4") {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "sysfs device vanished",
            ));
        }
        resolve(path)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains(ENV_KEY));
    assert!(error.to_string().contains("/dev/dri/card4"));
    assert!(error.to_string().contains("sysfs device vanished"));
}

#[test]
fn ambiguous_identity_fails_instead_of_selecting_the_first_match() {
    let error = StartupRendererPreference::from_value(Some(NVIDIA.into()), &seat_devices(), |_| {
        Ok(NVIDIA.into())
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn inactive_start_defers_opening_and_retains_identity_through_card_renumbering() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    assert!(
        preference
            .opening_order(&devices, false, |_| {
                panic!("inactive seat must defer opening and identity refresh")
            })
            .is_empty()
    );
    assert!(preference.pending.is_some());

    // Activation replaces the entire list. The old card8 now identifies AMD;
    // NVIDIA has a new card number and device ID, and enumeration order changed.
    let activated = [
        device(2, "/dev/dri/card2"),
        device(8, "/dev/dri/card8"),
        device(11, "/dev/dri/card11"),
    ];
    let ordered = preference.opening_order(&activated, true, |path| {
        Ok(match path.to_str().unwrap() {
            "/dev/dri/card2" => OTHER,
            "/dev/dri/card8" => AMD,
            "/dev/dri/card11" => NVIDIA,
            unexpected => panic!("unexpected device {unexpected}"),
        }
        .into())
    });
    assert_eq!(ids(&ordered), [11, 2, 8]);
    assert_eq!(preference.scene_after_open(None, 11, NVIDIA), 11);
    assert!(preference.pending.is_none());
}

#[test]
fn failed_preferred_falls_back_and_never_preempts_on_return_or_reselects_on_resume() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    let ordered = preference.opening_order(&devices, true, resolve);
    preference.open_failed(
        ordered[0].device_id,
        "DRM device initialization failed",
        &"denied",
    );
    assert_eq!(
        fallback_reason(&preference),
        "DRM device initialization failed: denied"
    );
    let fallback = preference.scene_after_open(None, ordered[1].device_id, AMD);
    assert_eq!(fallback, 4);
    assert!(preference.pending.is_none());
    assert_eq!(preference.scene_after_open(Some(fallback), 8, NVIDIA), 4);

    // After selection, even removing the canonical GPU follows normal failover
    // order; the preference must not revive on resume or on later hotplug.
    let remaining = [devices[1].clone(), devices[2].clone()];
    let ordered = preference.opening_order(&remaining, true, |_| {
        panic!("consumed preference must not resolve identities again")
    });
    assert_eq!(ids(&ordered), [2, 8]);
    assert_eq!(
        preference.scene_after_open(None, ordered[0].device_id, OTHER),
        2
    );
}

#[test]
fn notifier_failure_does_not_consume_preference_until_a_renderer_is_selected() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    preference.opening_order(&devices, true, resolve);
    preference.open_failed(8, "DRM notifier registration failed", &"event loop closed");
    assert_eq!(
        fallback_reason(&preference),
        "DRM notifier registration failed: event loop closed"
    );
    // Failure of another GPU must not overwrite the preferred failure reason.
    preference.open_failed(4, "DRM device initialization failed", &"unavailable");
    assert_eq!(
        fallback_reason(&preference),
        "DRM notifier registration failed: event loop closed"
    );
    assert_eq!(preference.scene_after_open(None, 2, OTHER), 2);
    assert!(preference.pending.is_none());
}

#[test]
fn all_failed_attempts_keep_preference_for_later_recovery() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    preference.opening_order(&devices, true, resolve);
    preference.open_failed(8, "DRM device initialization failed", &"not ready");
    preference.open_failed(4, "DRM device initialization failed", &"not ready");
    preference.open_failed(2, "DRM device initialization failed", &"not ready");
    assert!(preference.pending.is_some());
    let ordered = preference.opening_order(&devices, true, resolve);
    assert_eq!(ids(&ordered), [8, 4, 2]);
    assert!(
        preference
            .pending
            .as_ref()
            .unwrap()
            .fallback_reason
            .is_none()
    );
    assert_eq!(
        preference.scene_after_open(None, ordered[0].device_id, NVIDIA),
        8
    );
    assert!(preference.pending.is_none());
}

#[test]
fn disappearing_preferred_gpu_keeps_remaining_order_and_consumes_on_fallback() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    let remaining = [devices[1].clone(), devices[0].clone()];
    let ordered = preference.opening_order(&remaining, true, resolve);
    assert_eq!(ids(&ordered), [2, 4]);
    assert!(fallback_reason(&preference).contains("does not match any enumerated seat GPU"));
    assert_eq!(
        preference.scene_after_open(None, ordered[0].device_id, OTHER),
        2
    );
    assert_eq!(preference.scene_after_open(Some(2), 8, NVIDIA), 2);
    assert!(preference.pending.is_none());
}

#[test]
fn unresolved_identity_after_validation_keeps_automatic_fallback_available() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    let ordered = preference.opening_order(&devices, true, |path| {
        if path == Path::new("/dev/dri/card8") {
            return Err(io::Error::new(io::ErrorKind::NotFound, "GPU disappeared"));
        }
        resolve(path)
    });
    assert_eq!(ids(&ordered), [4, 2, 8]);
    assert!(fallback_reason(&preference).contains("GPU disappeared"));
    assert_eq!(
        preference.scene_after_open(None, ordered[0].device_id, AMD),
        4
    );
    assert!(preference.pending.is_none());
}

#[test]
fn ambiguity_after_validation_keeps_full_automatic_order() {
    let devices = seat_devices();
    let mut preference = preferred(&devices);
    let ordered = preference.opening_order(&devices, true, |_| Ok(NVIDIA.into()));
    assert_eq!(ids(&ordered), [4, 2, 8]);
    assert!(fallback_reason(&preference).contains("ambiguous"));
    assert_eq!(preference.scene_after_open(None, 4, AMD), 4);
    assert!(preference.pending.is_none());
}
