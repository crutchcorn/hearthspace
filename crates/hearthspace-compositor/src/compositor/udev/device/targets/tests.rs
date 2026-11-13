use super::*;

fn mode(clock: u32, flags: u32) -> Mode {
    drm_ffi::drm_mode_modeinfo {
        clock,
        hdisplay: 1920,
        hsync_start: 2008,
        hsync_end: 2052,
        htotal: 2200,
        vdisplay: 1080,
        vsync_start: 1084,
        vsync_end: 1089,
        vtotal: 1125,
        vrefresh: 60,
        flags,
        ..Default::default()
    }
    .into()
}

#[test]
fn display_modes_identity_preserves_timing_flags_and_fractional_refresh() {
    let native = mode(148500, 0);
    let other_flags = mode(148500, 1);
    let fractional = mode(148352, 0);
    assert_ne!(native, other_flags);
    assert_ne!(
        KmsOutputTarget::describe_mode(native).id,
        KmsOutputTarget::describe_mode(other_flags).id
    );
    assert_eq!(
        KmsOutputTarget::describe_mode(native).refresh_millihertz,
        60000
    );
    assert_eq!(
        KmsOutputTarget::describe_mode(fractional).refresh_millihertz,
        59940
    );
    assert_ne!(
        KmsOutputTarget::describe_mode(native).id,
        KmsOutputTarget::describe_mode(fractional).id
    );
}

#[test]
fn display_modes_confirmed_timing_is_selected_exactly_and_missing_timing_falls_back() {
    use super::super::super::modes::State;
    let native = mode(148500, 0);
    let fractional = mode(148352, 0);
    let mut outputs = [KmsOutputTarget {
        connector: std::num::NonZeroU32::new(1).unwrap().into(),
        crtc: std::num::NonZeroU32::new(2).unwrap().into(),
        mode: native,
        modes: vec![native, fractional],
        connector_name: "eDP-1".into(),
        connector_label: "eDP-1".into(),
        physical_size_mm: (0, 0),
    }];
    let mut catalog_change = outputs[0].clone();
    catalog_change.modes = vec![native];
    assert_eq!(
        catalog_change, outputs[0],
        "catalog-only changes must preserve healthy scanout"
    );
    catalog_change.mode = fractional;
    assert_ne!(
        catalog_change, outputs[0],
        "selected timing changes must reconfigure scanout"
    );
    let mut settings = State::default();
    settings.selected.insert(
        "eDP-1".into(),
        KmsOutputTarget::describe_mode(fractional).id,
    );
    settings.select(&mut outputs);
    assert_eq!(outputs[0].mode, fractional);
    outputs[0].mode = native;
    outputs[0].modes = vec![native];
    settings.select(&mut outputs);
    assert_eq!(outputs[0].mode, native);
    assert_eq!(
        settings.selected["eDP-1"],
        KmsOutputTarget::describe_mode(fractional).id,
        "disconnected preferences are retained"
    );
}
