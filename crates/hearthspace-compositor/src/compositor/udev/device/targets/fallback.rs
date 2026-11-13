//! Driver-scaled common modes. libxcvt generates timings; DRM validates them.
use smithay::{
    backend::drm::DrmDeviceFd,
    reexports::drm::control::{Device, Mode, connector},
};

#[allow(non_camel_case_types, non_upper_case_globals, dead_code)]
mod ffi {
    include!(concat!(env!("OUT_DIR"), "/xcvt.rs"));
}

pub(super) fn append(
    fd: &DrmDeviceFd,
    connector: connector::Handle,
    interface: connector::Interface,
    modes: &mut Vec<Mode>,
) -> std::io::Result<()> {
    let properties = fd.get_properties(connector)?;
    let (handles, values) = properties.as_props_and_values();
    let mut scaling = false;
    let mut tiled = false;
    for (&handle, &value) in handles.iter().zip(values) {
        let info = fd.get_property(handle)?;
        match info.name().to_bytes() {
            b"scaling mode" => scaling = true,
            b"TILE" if value != 0 => tiled = true,
            _ => {}
        }
    }
    if scaling && !tiled {
        append_common(
            modes,
            interface == connector::Interface::EmbeddedDisplayPort,
        );
    }
    Ok(())
}

fn append_common(modes: &mut Vec<Mode>, edp: bool) {
    let Some(first) = modes.first().copied() else {
        return;
    };
    if edp && modes.iter().any(|m| m.size() != first.size()) {
        return;
    }
    let native = modes.clone();
    let max_w = native.iter().map(|m| m.size().0).max().unwrap();
    let max_h = native.iter().map(|m| m.size().1).max().unwrap();
    let max_clock = native.iter().map(Mode::clock).max().unwrap();
    let rates: Vec<_> = native
        .iter()
        .map(|m| smithay::output::Mode::from(*m).refresh)
        .collect();
    let sizes = [
        (640, 480),
        (800, 600),
        (1024, 768),
        (1152, 864),
        (1280, 720),
        (1280, 768),
        (1280, 800),
        (1280, 960),
        (1280, 1024),
        (1360, 768),
        (1366, 768),
        (1440, 900),
        (1600, 900),
        (1600, 1200),
        (1680, 1050),
        (1920, 1080),
        (1920, 1200),
        (2560, 1440),
        (2560, 1600),
        (3840, 2160),
    ];
    for (w, h) in sizes {
        let (w, h) = if max_w < max_h { (h, w) } else { (w, h) };
        if w > i32::from(max_w)
            || h > i32::from(max_h)
            || (w == i32::from(max_w) && h == i32::from(max_h))
        {
            continue;
        }
        for &rate in &rates {
            // Prefer reduced blanking on fixed panels, but retain only one
            // candidate per supported rate/resolution after clock validation.
            let candidate = [true, false]
                .into_iter()
                .filter_map(|reduced| cvt(w, h, rate, reduced))
                .filter(|m| {
                    let actual = smithay::output::Mode::from(*m).refresh;
                    m.clock() <= max_clock && rates.iter().any(|r| (actual - r).abs() < 500)
                })
                .min_by_key(|m| (smithay::output::Mode::from(*m).refresh - rate).abs());
            if let Some(mode) = candidate {
                let refresh = smithay::output::Mode::from(mode).refresh;
                if !modes.iter().any(|m| {
                    m.size() == mode.size()
                        && (smithay::output::Mode::from(*m).refresh - refresh).abs() < 1
                }) {
                    modes.push(mode);
                }
            }
        }
    }
}

fn cvt(width: i32, height: i32, rate: i32, reduced: bool) -> Option<Mode> {
    // Inputs are bounded common dimensions and a positive kernel-derived rate.
    if !(10_000..=1_000_000).contains(&rate) {
        return None;
    }
    let pointer = unsafe {
        ffi::libxcvt_gen_mode_info(width, height, (rate as f32 / 1000.).round(), reduced, false)
    };
    if pointer.is_null() {
        return None;
    }
    // The library returns one malloc-owned public mode_info record. The guard
    // frees it on conversion failure as well as on the successful path.
    struct Owned(*mut ffi::libxcvt_mode_info);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                ffi::free(self.0.cast());
            }
        }
    }
    let owned = Owned(pointer);
    let info = unsafe { &*owned.0 };
    let mut raw = drm_ffi::drm_mode_modeinfo {
        clock: u32::try_from(info.dot_clock).ok()?,
        hdisplay: info.hdisplay.try_into().ok()?,
        vdisplay: info.vdisplay.try_into().ok()?,
        hsync_start: info.hsync_start,
        hsync_end: info.hsync_end,
        htotal: info.htotal,
        vsync_start: info.vsync_start,
        vsync_end: info.vsync_end,
        vtotal: info.vtotal,
        flags: info.mode_flags,
        type_: 1 << 5,
        vrefresh: ((rate + 500) / 1000) as u32,
        ..Default::default()
    };
    for (byte, source) in raw
        .name
        .iter_mut()
        .zip(format!("{}x{}", raw.hdisplay, raw.vdisplay).bytes())
    {
        *byte = source as _;
    }
    Some(raw.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn native() -> Mode {
        cvt(1920, 1080, 60000, true).unwrap()
    }
    #[test]
    fn common_modes_stay_within_native_dimensions_clock_and_refresh() {
        let original = native();
        let mut modes = vec![original];
        append_common(&mut modes, true);
        assert!(modes.iter().any(|m| m.size() == (1280, 720)));
        for mode in modes {
            assert!(
                mode.size().0 <= 1920 && mode.size().1 <= 1080 && mode.clock() <= original.clock()
            );
            assert!(
                (smithay::output::Mode::from(mode).refresh
                    - smithay::output::Mode::from(original).refresh)
                    .abs()
                    < 500
            );
        }
    }
    #[test]
    fn edp_with_existing_resolution_choices_is_left_unchanged() {
        let mut modes = vec![native(), cvt(1280, 720, 60000, true).unwrap()];
        let original = modes.clone();
        append_common(&mut modes, true);
        assert_eq!(modes, original);
    }
}
