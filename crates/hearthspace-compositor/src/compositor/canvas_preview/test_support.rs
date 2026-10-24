use super::*;

/// The wall clock has passed the endpoint, but no new frame has been sampled.
pub(in crate::compositor) fn held_preview() -> (State, Instant) {
    let now = Instant::now();
    let started = now - DURATION - Duration::from_millis(40);
    let mut state = State {
        sampled_at: started + Duration::from_millis(120),
        ..Default::default()
    };
    state.poses.insert(
        7,
        Pose {
            from: Rect {
                x: 10.25,
                y: 40.75,
                width: 80.5,
                height: 60.25,
            },
            to: Rect {
                x: 510.25,
                y: 240.75,
                width: 80.5,
                height: 60.25,
            },
            natural: Rect::default(),
            from_opacity: 1.,
            to_opacity: 0.4,
            started,
            matches: true,
            reveal: false,
        },
    );
    (state, now)
}
