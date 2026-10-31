//! A failed legacy clear on one CRTC cannot hold another cursor visible.
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
    frames: [CursorFrame; 2],
    owner: u32,
    software: bool,
    wakes: usize,
}

impl Fixture {
    fn new() -> Self {
        let mut plane = CursorPlane::default();
        let mut drm = Drm::default();
        assert!(!plane.update(target(1), false, Scene::default(), &mut drm));
        drm.clear_errors.insert(target(1).crtc);
        assert!(!plane.update(target(2), false, Scene::default(), &mut drm));
        assert_eq!(drm.visible, [target(1).crtc, target(2).crtc].into());
        assert!(plane.clear_retry_pending());
        Self {
            plane,
            drm,
            frames: Default::default(),
            owner: 2,
            software: false,
            wakes: 0,
        }
    }

    fn update(&mut self) -> bool {
        self.plane.update(
            target(self.owner),
            self.software,
            scene(
                &[&self.frames[0], &self.frames[1]],
                (self.owner - 1) as usize,
            ),
            &mut self.drm,
        )
    }

    fn retry(&mut self) -> bool {
        self.wakes += 1;
        self.plane.retry_clear();
        self.update();
        self.plane.clear_retry_pending()
    }

    fn clears(&self, crtc: u32) -> usize {
        self.drm
            .calls
            .iter()
            .filter(|op| *op == &Op::Clear(target(crtc).crtc))
            .count()
    }
}

fn tick(event_loop: &mut EventLoop<'static, Fixture>, fixture: &mut Fixture) {
    let before = fixture.wakes;
    event_loop
        .dispatch(Some(Duration::from_secs(1)), fixture)
        .unwrap();
    assert_eq!(fixture.wakes, before + 1);
}

fn stopped(event_loop: &mut EventLoop<'static, Fixture>, fixture: &mut Fixture) {
    assert!(!fixture.plane.clear_retry_pending());
    let before = (fixture.wakes, fixture.drm.calls.len());
    event_loop
        .dispatch(Some(INTERVAL + Duration::from_millis(10)), fixture)
        .unwrap();
    assert_eq!(
        (fixture.wakes, fixture.drm.calls.len()),
        before,
        "cleanup timer must be removed past its interval"
    );
}

#[test]
fn a_only_clear_failure_cannot_block_bs_presented_software_replacement() {
    let mut fixture = Fixture::new();
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    fixture.software = true;
    fixture.frames[1].queued(true);
    assert!(fixture.update());
    tick(&mut event_loop, &mut fixture);
    assert_eq!(
        fixture.clears(2),
        0,
        "retain B until its replacement presents"
    );

    fixture.frames[1].completed(true);
    assert!(fixture.update());
    assert!(fixture.plane.active.is_none());
    assert_eq!(fixture.drm.visible, [target(1).crtc].into());
    assert_eq!(fixture.clears(2), 1, "healthy B clears despite blocked A");
    let attempts = fixture.clears(1);
    for _ in 0..20 {
        assert!(fixture.update());
    }
    assert_eq!(
        fixture.clears(1),
        attempts,
        "ordinary updates cannot retry A"
    );
    for _ in 0..3 {
        tick(&mut event_loop, &mut fixture);
    }
    assert_eq!(
        fixture.clears(1),
        5,
        "A gets one initial attempt plus four retries"
    );
    assert_eq!(fixture.clears(2), 1);
    assert_eq!(fixture.drm.visible, [target(1).crtc].into());
    assert!(
        !fixture.plane.clear(&mut fixture.drm),
        "exhausted A remains tracked"
    );
    stopped(&mut event_loop, &mut fixture);
}

#[test]
fn failed_a_retries_do_not_clear_b_reused_after_superseded_software() {
    let mut fixture = Fixture::new();
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    fixture.frames[1].queued(true);
    fixture.software = true;
    assert!(fixture.update());
    fixture.software = false;
    assert!(!fixture.update());
    tick(&mut event_loop, &mut fixture);
    assert_eq!(fixture.clears(2), 0);
    assert_eq!(fixture.plane.active.unwrap().crtc, target(2).crtc);

    fixture.frames[1].completed(true);
    assert!(!fixture.update());
    assert!(
        fixture.plane.active.is_none(),
        "superseded software pixels still replace B"
    );
    fixture.frames[1].queued(false);
    assert!(!fixture.update());
    assert!(fixture.plane.active.is_none());
    fixture.frames[1].completed(true);
    assert!(!fixture.update());
    assert_eq!(fixture.plane.active.unwrap().crtc, target(2).crtc);
    for _ in 0..3 {
        tick(&mut event_loop, &mut fixture);
    }
    assert_eq!(fixture.clears(1), 5);
    assert_eq!(
        fixture.clears(2),
        1,
        "only the actual B software presentation retires B"
    );
    assert_eq!(fixture.drm.visible, [target(1).crtc, target(2).crtc].into());
    stopped(&mut event_loop, &mut fixture);

    // Returning to A cannot silently reuse its exhausted cleanup ownership.
    fixture.drm.clear_errors.remove(&target(1).crtc);
    fixture.owner = 1;
    assert!(fixture.update());
    assert_eq!(fixture.clears(1), 5);
    assert_eq!(fixture.plane.active.unwrap().crtc, target(2).crtc);
}

#[test]
fn returning_to_retired_a_can_retire_b_when_as_software_presents() {
    let mut fixture = Fixture::new();
    fixture.owner = 1;
    fixture.software = true;
    fixture.frames[0].queued(true);
    assert!(fixture.update());
    assert_eq!(fixture.clears(2), 0);
    fixture.frames[0].completed(true);
    assert!(fixture.update());
    assert_eq!(fixture.clears(2), 1);
    assert!(fixture.plane.active.is_none());
    assert_eq!(fixture.drm.visible, [target(1).crtc].into());
    assert_eq!(fixture.clears(1), 1);
    assert!(fixture.plane.clear_retry_pending());
}

#[test]
fn exhausted_a_does_not_consume_bs_later_independent_retry_budget() {
    let mut fixture = Fixture::new();
    let mut event_loop = EventLoop::try_new().unwrap();
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    for _ in 0..4 {
        tick(&mut event_loop, &mut fixture);
    }
    stopped(&mut event_loop, &mut fixture);
    assert_eq!(fixture.clears(1), 5);
    fixture.drm.clear_errors.insert(target(2).crtc);
    fixture.frames[1].queued(true);
    fixture.frames[1].completed(true);
    fixture.software = true;
    assert!(fixture.update());
    assert_eq!(fixture.clears(2), 1);
    assert!(fixture.plane.clear_retry_pending());
    insert_retry(&event_loop.handle(), Fixture::retry).unwrap();
    for _ in 0..4 {
        tick(&mut event_loop, &mut fixture);
    }
    assert_eq!(
        fixture.clears(1),
        5,
        "new B cleanup must not reopen A's budget"
    );
    assert_eq!(fixture.clears(2), 5);
    assert_eq!(fixture.wakes, 8);
    assert_eq!(fixture.drm.visible, [target(1).crtc, target(2).crtc].into());
    assert!(fixture.plane.active.is_some());
    let attempts = fixture.drm.calls.len();
    assert!(fixture.update());
    assert_eq!(fixture.drm.calls.len(), attempts);
    stopped(&mut event_loop, &mut fixture);
}
