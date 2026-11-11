//! Real lock/output-sync effects with a headless renderer and injected completion.
use super::*;
use crate::compositor::{
    App, OutputDescriptor,
    frame_callbacks::test_support::app::new_app,
    runtime::{HeadlessBackend, create_headless_calloop_data},
    udev::output_sync::PublishedOutputs,
};
use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{Offscreen, gles::GlesRenderer},
    },
    output::{PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
};
use wayland_server::Display;

fn fixture() -> (
    CalloopData,
    PublishedOutputs,
    EventLoop<'static, CalloopData>,
) {
    let display = Display::<App>::new().unwrap();
    let events = EventLoop::try_new().unwrap();
    let app = new_app(&display.handle(), events.handle());
    let egl = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&egl).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
    let size = (2, 2).into();
    let buffer = renderer
        .create_buffer(Fourcc::Abgr8888, (2, 2).into())
        .unwrap();
    let mut data = create_headless_calloop_data(
        app,
        display,
        HeadlessBackend { renderer, buffer },
        size,
        None,
    );
    let mut published = PublishedOutputs::default();
    let output = OutputDescriptor {
        name: "eDP-1".into(),
        properties: PhysicalProperties {
            size: (300, 200).into(),
            subpixel: Subpixel::Unknown,
            make: "test".into(),
            model: "panel".into(),
            serial_number: "test".into(),
        },
        size,
        scale: 1,
        refresh: 60_000,
    };
    apply_connector_sync(&mut data, published.update(vec![output]));
    data.state.set_native_lock_outputs_available(true);
    data.state.arm_session_shield();
    let first = data.state.lock_frame_generation().unwrap();
    data.state.lock_frame_presented("eDP-1", first);
    assert!(data.state.lock_confirmed());
    // A real resume invalidates the pre-sleep completion once.
    data.state.invalidate_lock_presentation();
    assert!(!data.state.lock_confirmed());
    (data, published, events)
}

#[test]
fn unchanged_recovery_retry_accepts_delayed_lock_flip_without_restarting_generation() {
    let (mut data, mut published, _events) = fixture();
    let in_flight = data.state.lock_frame_generation().unwrap();
    assert_eq!(data.state.capture_outputs().len(), 1);
    let retained = std::collections::BTreeSet::from(["eDP-1".into()]);
    // Timer ticks while the first resumed frame is still in flight. No topology
    // or scanout-source change has occurred, so its generation remains valid.
    for _ in 0..3 {
        apply_recovery_retry(&mut data, published.update_pending(vec![], &retained));
        assert_eq!(data.state.lock_frame_generation(), Some(in_flight));
    }
    data.state.lock_frame_presented("eDP-1", in_flight);
    assert!(data.state.lock_confirmed());
}

#[test]
fn replaced_scanout_still_rejects_old_lock_flip_and_requires_fresh_completion() {
    let (mut data, mut published, _events) = fixture();
    let old = data.state.lock_frame_generation().unwrap();
    published.invalidate(["eDP-1"]);
    // Replacement of a physical source retains its Wayland output identity.
    let sync =
        published.update_pending(vec![], &std::collections::BTreeSet::from(["eDP-1".into()]));
    apply_recovery_retry(&mut data, sync);
    let fresh = data.state.lock_frame_generation().unwrap();
    assert_ne!(old, fresh);
    data.state.lock_frame_presented("eDP-1", old);
    assert!(!data.state.lock_confirmed());
    data.state.set_native_lock_outputs_available(true);
    data.state.lock_frame_presented("eDP-1", fresh);
    assert!(data.state.lock_confirmed());
}
