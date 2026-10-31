//! Real calloop timer dispatch progresses cleanup without input or new frames.
use super::*;
use crate::compositor::udev::{
    cursor::retry::{INTERVAL, insert_retry},
    device::cursor::policy::CursorPlane,
};
use smithay::reexports::calloop::EventLoop;
use std::time::Duration;

struct Fixture {
    plane: CursorPlane,
    drm: Drm,
    target: Target,
    scene: Scene,
    software: bool,
    wakes: usize,
}

impl Fixture {
    fn new(migration: bool) -> Self {
        let mut plane = CursorPlane::default();
        let mut drm = Drm::default();
        plane.update(target(1), false, Scene::default(), &mut drm);
        let mut fixture = Self {
            plane,
            drm,
            target: target(if migration { 2 } else { 1 }),
            scene: Scene {
                software_displayed: !migration,
                replacement_displayed: !migration,
                ..Default::default()
            },
            software: !migration,
            wakes: 0,
        };
        fixture.drm.clear_error = true;
        fixture.update();
        assert!(fixture.plane.clear_retry_pending());
        fixture
    }

    fn update(&mut self) {
        assert_eq!(
            self.plane
                .update(self.target, self.software, self.scene, &mut self.drm),
            self.software,
        );
    }

    fn retry(&mut self) -> bool {
        self.wakes += 1;
        self.plane.retry_clear();
        self.update();
        self.plane.clear_retry_pending()
    }
}

#[rstest::rstest]
#[case(false)]
#[case(true)]
fn idle_clear_failure_recovers_from_real_timer_without_manual_update(#[case] migration: bool) {
    let mut fixture = Fixture::new(migration);
    let failed_calls = fixture.drm.calls.len();
    for _ in 0..20 {
        fixture.update();
    }
    assert_eq!(
        fixture.drm.calls.len(),
        failed_calls,
        "motion cannot bypass retry spacing"
    );
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    fixture.drm.clear_error = false;
    // No new pointer event, redraw or page flip is supplied to drive the retry.
    event_loop
        .dispatch(Some(Duration::from_secs(1)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 1);
    assert!(!fixture.plane.clear_retry_pending());
    let expected = if migration {
        vec![target(2).crtc]
    } else {
        vec![]
    };
    assert_eq!(
        fixture.drm.visible,
        expected
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
    );
    event_loop
        .dispatch(Some(INTERVAL + Duration::from_millis(10)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 1, "successful cleanup removes its timer");
}

#[test]
fn persistent_clear_failure_exhausts_timer_and_lifecycle_restores_retry_budget() {
    let mut fixture = Fixture::new(false);
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    for _ in 0..4 {
        event_loop
            .dispatch(Some(Duration::from_secs(1)), &mut fixture)
            .unwrap();
    }
    assert_eq!(fixture.wakes, 4);
    assert!(!fixture.plane.clear_retry_pending());
    assert!(
        fixture.plane.active.is_some(),
        "failed cleanup must retain its owner"
    );
    let exhausted_calls = fixture.drm.calls.len();
    fixture.update();
    event_loop
        .dispatch(Some(INTERVAL + Duration::from_millis(10)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 4);
    assert_eq!(
        fixture.drm.calls.len(),
        exhausted_calls,
        "exhaustion cannot busy-spin"
    );

    fixture.plane.reset();
    fixture
        .plane
        .initialize([target(1).crtc].into_iter(), &mut fixture.drm);
    assert!(
        fixture.plane.clear_retry_pending(),
        "new lifecycle permits bounded cleanup again"
    );
}

#[test]
fn timer_rechecks_current_intent_instead_of_clearing_a_reused_cursor() {
    let mut fixture = Fixture::new(false);
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    // The original request to retire hardware has been superseded, and the
    // cursor-free primary frame has completed before the delayed retry.
    fixture.software = false;
    fixture.scene = Scene::default();
    fixture.drm.clear_error = false;
    let previous_calls = fixture.drm.calls.len();
    event_loop
        .dispatch(Some(Duration::from_secs(1)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 1);
    assert_eq!(fixture.drm.calls.len(), previous_calls);
    assert_eq!(fixture.drm.visible, [target(1).crtc].into());
    assert!(fixture.plane.active.is_some());
    assert!(!fixture.plane.clear_retry_pending());
    event_loop
        .dispatch(Some(INTERVAL + Duration::from_millis(10)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 1, "superseded cleanup removes its timer");
    assert_eq!(fixture.drm.calls.len(), previous_calls);
}

#[test]
fn cancelled_timer_cannot_touch_a_paused_device() {
    let mut fixture = Fixture::new(false);
    let mut event_loop = EventLoop::try_new().unwrap();
    let handle = event_loop.handle();
    let token = insert_retry(&handle, Fixture::retry).unwrap();
    handle.remove(token); // same source removal used by pause and backend drop
    fixture.plane.reset();
    event_loop
        .dispatch(Some(INTERVAL + Duration::from_millis(10)), &mut fixture)
        .unwrap();
    assert_eq!(fixture.wakes, 0);
    assert!(fixture.plane.active.is_none());
    assert!(!fixture.plane.clear_retry_pending());
}
