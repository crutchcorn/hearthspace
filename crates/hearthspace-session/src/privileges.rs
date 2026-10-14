//! A desktop session needs ordinary user credentials, not PAM ambient caps.
//! In particular, pam_systemd can grant CAP_WAKE_ALARM on login. Inheriting it
//! makes Bubblewrap reject GTK's sandboxed image loaders before any UI appears.

use caps::CapSet;

/// Run before creating threads, the private D-Bus daemon, or any child process.
/// Keep the bounding set intact: explicit sudo/polkit authentication and normal
/// setuid helpers still work. This only discards the session's inherited grants.
pub(crate) fn clear_inherited() -> Result<(), caps::errors::CapsError> {
    for set in [
        CapSet::Ambient,
        CapSet::Inheritable,
        CapSet::Effective,
        CapSet::Permitted,
    ] {
        caps::clear(None, set)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_kernel_thread_capabilities_without_changing_bounding_set() {
        // Capability sets are per-thread; do not mutate the test runner thread.
        std::thread::spawn(|| {
            let bounding = caps::read(None, CapSet::Bounding).unwrap();
            clear_inherited().unwrap();
            clear_inherited().unwrap();
            for set in [
                CapSet::Ambient,
                CapSet::Inheritable,
                CapSet::Effective,
                CapSet::Permitted,
            ] {
                assert!(caps::read(None, set).unwrap().is_empty(), "{set:?}");
            }
            assert_eq!(caps::read(None, CapSet::Bounding).unwrap(), bounding);
        })
        .join()
        .unwrap();
    }
}
