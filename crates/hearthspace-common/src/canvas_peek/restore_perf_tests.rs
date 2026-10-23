use super::{Work, restore};
use crate::canvas_peek::{tests::*, *};

#[test]
fn a_blocked_chain_of_4096_homes_has_quadratic_work() {
    const COUNT: usize = 4096;
    let mut scene = Scene {
        decorations: (0..COUNT)
            .map(|index| Decoration {
                home: Some(point(index as f64 * 128.0, 0.0)),
                ..icon(index as u64, (index as f64 - 1.0) * 128.0, 0.0)
            })
            .collect(),
        windows: vec![window(
            COUNT as u64,
            rect((COUNT - 1) as f64 * 128.0, 0.0, 88.0, 96.0),
        )],
        ..Scene::default()
    };
    let original = scene.clone();
    let mut work = Work::default();
    restore(&mut scene, &mut work);
    assert_eq!(scene, original);
    // The old ascending rescans eliminate only one predecessor per pass and
    // perform Θ(N³) peer checks. Count actual loop work, never wall time.
    assert!(work.steps <= 4 * COUNT * COUNT, "{} steps", work.steps);
    assert_eq!(work.removed, 0);
    assert_eq!(work.rounds, 1);
}

#[test]
fn a_large_cycle_returns_simultaneously_with_quadratic_work() {
    const COUNT: usize = 512;
    let homes: Vec<_> = (0..COUNT)
        .map(|index| icon(index as u64, index as f64 * 128.0, 0.0))
        .collect();
    let mut scene = Scene {
        decorations: (0..COUNT)
            .map(|index| Decoration {
                home: Some(position(homes[index].bounds)),
                ..icon(index as u64, ((index + 1) % COUNT) as f64 * 128.0, 0.0)
            })
            .collect(),
        ..Scene::default()
    };
    let mut work = Work::default();
    restore(&mut scene, &mut work);
    assert_eq!(scene.decorations, homes);
    assert!(work.steps <= 7 * COUNT * COUNT, "{} steps", work.steps);
    assert_eq!(work.removed, 1);
    assert_eq!(work.rounds, 1);
}

#[test]
fn an_individual_fallback_releases_a_cycle_in_the_next_batch() {
    let displaced = |id, current, home| Decoration {
        home: Some(point(home, 0.0)),
        ..control(id, rect(current, 0.0, 1.0, 1.0), false)
    };
    let blocked = displaced(4, 10.0, 4.5);
    let mut scene = Scene {
        decorations: vec![
            displaced(1, 0.0, 4.0),
            displaced(2, 2.0, 0.0),
            displaced(3, 0.0, 2.0),
            blocked,
        ],
        windows: vec![window(5, rect(5.1, 0.0, 0.3, 1.0))],
        ..Scene::default()
    };
    let mut work = Work::default();
    restore(&mut scene, &mut work);
    assert_eq!(
        scene.decorations,
        [
            control(1, rect(4.0, 0.0, 1.0, 1.0), false),
            control(2, rect(0.0, 0.0, 1.0, 1.0), false),
            control(3, rect(2.0, 0.0, 1.0, 1.0), false),
            blocked,
        ]
    );
    assert_eq!(work.rounds, 2);
}

#[test]
fn cached_rejection_matches_reference_order_for_overlapping_homes_and_layers() {
    let mut seed = 0xd9e5_7a21_614b_834f_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    // Dense, deliberately conflicting homes exercise rejection order, changed
    // blockers, reverse fallback and both directions of the layer filter.
    for case in 0..2000 {
        let mut scene = Scene::default();
        for id in 0..12 {
            let bounds = rect(
                (next() % 8) as f64 * 32.0,
                (next() % 4) as f64 * 32.0,
                32.0 + (next() % 4) as f64 * 24.0,
                32.0 + (next() % 4) as f64 * 24.0,
            );
            scene.decorations.push(Decoration {
                id,
                bounds,
                body: if next().is_multiple_of(2) {
                    bounds
                } else {
                    icon_body(bounds)
                },
                raised: next().is_multiple_of(3),
                home: (!next().is_multiple_of(5))
                    .then(|| point((next() % 8) as f64 * 32.0, (next() % 4) as f64 * 32.0)),
            });
        }
        scene.windows.push(Window {
            background: next().is_multiple_of(2),
            ..window(20, rect((next() % 8) as f64 * 32.0, 64.0, 48.0, 64.0))
        });
        scene.obstacles.push(Obstacle {
            background: next().is_multiple_of(2),
            bounds: rect(32.0, (next() % 4) as f64 * 32.0, 80.0, 20.0),
        });
        let mut expected = scene.clone();
        reference_restore(&mut expected);
        let mut work = Work::default();
        restore(&mut scene, &mut work);
        assert_eq!(scene, expected, "reference mismatch in case {case}");
        assert_quadratic(&work, scene.decorations.len(), 2);
    }
}

#[test]
fn repeated_reverse_fallback_does_not_rescan_an_unrelated_blocked_chain() {
    const LADDER: usize = 256;
    const CHAIN: usize = 512;
    let displaced = |id, x, y, home_x, home_y, height| Decoration {
        home: Some(point(home_x, home_y)),
        ..control(id, rect(x, y, 10.0, height), false)
    };
    let mut decorations: Vec<_> = (0..LADDER)
        .map(|i| {
            displaced(
                i as u64,
                (i + 1) as f64 * 100.0,
                0.0,
                i as f64 * 100.0,
                0.0,
                10.0,
            )
        })
        .collect();
    // Every ladder home conflicts with a later bait home. The thin window
    // blocks only the baits; reverse fallback frees one ladder member per round.
    decorations.extend((0..LADDER).map(|i| {
        displaced(
            (LADDER + i) as u64,
            i as f64 * 100.0,
            20000.0,
            i as f64 * 100.0,
            0.0,
            20.0,
        )
    }));
    decorations.extend((0..CHAIN).map(|i| {
        displaced(
            (2 * LADDER + i) as u64,
            (i as f64 - 1.0) * 100.0,
            1000.0,
            i as f64 * 100.0,
            1000.0,
            10.0,
        )
    }));
    decorations.push(control(
        10000,
        rect((CHAIN - 1) as f64 * 100.0, 1000.0, 10.0, 10.0),
        false,
    ));
    let mut scene = Scene {
        decorations,
        windows: vec![window(10001, rect(0.0, 15.0, LADDER as f64 * 100.0, 1.0))],
        ..Scene::default()
    };
    let mut expected = scene.clone();
    for item in &mut expected.decorations[..LADDER] {
        let home = item.home.take().unwrap();
        *item = item.at(home);
    }
    let mut work = Work::default();
    restore(&mut scene, &mut work);
    assert_eq!(scene, expected);
    assert_eq!(work.rounds, LADDER + 1);
    assert_eq!(work.removed, LADDER);
    assert_quadratic(&work, scene.decorations.len(), 1);
}

fn assert_quadratic(work: &Work, count: usize, windows: usize) {
    assert!(
        work.steps <= 20 * count * count + count * windows + 4 * count,
        "{} steps for {count} decorations",
        work.steps
    );
    assert!(work.removed <= count);
    assert!(work.raised_changes <= 2 * count);
    assert!(work.rounds <= count + 1);
}

// Deliberately retain the uncached reference algorithm as an independent oracle
// for the small differential scenes; never use it for the large work regressions.
fn reference_restore(scene: &mut Scene) {
    let windows = [
        covering_windows(scene, false),
        covering_windows(scene, true),
    ];
    let items = &mut scene.decorations;
    loop {
        let homes: Vec<_> = items
            .iter()
            .map(|item| {
                let mut home = item.home.map_or(*item, |position| item.at(position));
                home.home = None;
                home
            })
            .collect();
        let mut returning: Vec<_> = items.iter().map(|item| item.home.is_some()).collect();
        loop {
            let mut blocked = false;
            for (index, home) in homes.iter().enumerate() {
                if returning[index]
                    && reference_blocked(
                        *home,
                        &windows[usize::from(home.raised)],
                        items
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .map(|(other, item)| {
                                if returning[other] {
                                    homes[other]
                                } else {
                                    *item
                                }
                            }),
                    )
                {
                    returning[index] = false;
                    blocked = true;
                }
            }
            if !blocked {
                break;
            }
        }
        let mut restored = false;
        for (index, returns) in returning.into_iter().enumerate() {
            if returns {
                items[index] = homes[index];
                restored = true;
            }
        }
        for index in (0..items.len()).rev() {
            if items[index].home.is_some()
                && !reference_blocked(
                    homes[index],
                    &windows[usize::from(items[index].raised)],
                    items
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != index)
                        .map(|(_, item)| *item),
                )
            {
                items[index] = homes[index];
                restored = true;
            }
        }
        if !restored {
            break;
        }
    }
}

fn reference_blocked(
    home: Decoration,
    windows: &[Rect],
    peers: impl Iterator<Item = Decoration>,
) -> bool {
    windows
        .iter()
        .any(|other| geometry::intersect(home.bounds, *other).is_some())
        || peers
            .filter(|other| !home.raised || other.raised)
            .any(|other| geometry::intersect(home.body, other.body).is_some())
}
