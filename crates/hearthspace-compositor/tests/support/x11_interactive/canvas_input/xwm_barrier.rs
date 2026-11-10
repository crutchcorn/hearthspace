//! Observe Satellite dispatch without changing focus or stacking.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn xwm_barrier(
    compositor: &HeadlessCompositor,
    observer: &RustConnection,
    requestor: Window,
    managed: u64,
) {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let title = format!(
        "Satellite dispatch {}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    // A root-server roundtrip alone cannot observe the independent XWM.
    // Satellite forwards this title through XDG; its appearance in the host
    // proves dispatch of preceding X events, without a stacking request.
    // This is NOT a Wayland/MI input-ingress fence; deferred-input tests use
    // an actual delivered event on an unfrozen device for that boundary.
    observer
        .change_property8(
            PropMode::REPLACE,
            requestor,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )
        .unwrap();
    observer.flush().unwrap();
    wait_windows(compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == managed && w["title"] == title)
    });
}
