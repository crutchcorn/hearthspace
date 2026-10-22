use super::{tests::*, *};

#[test]
fn repeated_peeks_keep_the_first_home_and_restore_both_body_and_bounds() {
    let original = icon(1, 100.0, 100.0);
    let mut scene = Scene {
        windows: vec![window(2, rect(100.0, 100.0, 200.0, 200.0))],
        decorations: vec![original],
        ..Scene::default()
    };
    settle(&mut scene, world());
    let first = scene.decorations[0];
    scene.windows[0].bounds.y = 0.0;
    settle(&mut scene, world());
    assert_ne!(scene.decorations[0].bounds, first.bounds);
    assert_eq!(scene.decorations[0].home, Some(point(100.0, 100.0)));
    scene.windows.clear();
    settle(&mut scene, world());
    assert_eq!(scene.decorations, [original]);
}

#[test]
fn a_window_covering_only_the_home_label_prevents_return() {
    let mut scene = Scene {
        windows: vec![window(1, rect(100.0, 100.0, 200.0, 200.0))],
        decorations: vec![icon(2, 100.0, 100.0)],
        ..Scene::default()
    };
    settle(&mut scene, world());
    scene.windows[0].bounds.y = 195.0;
    let displaced = scene.clone();
    restore_background(&mut scene);
    assert_eq!(scene, displaced);
    settle(&mut scene, world());
    assert_eq!(scene, displaced);
}

#[test]
fn nearby_icon_padding_does_not_prevent_a_return_after_repeated_peeks() {
    let originals = vec![icon(1, 100.0, 100.0), icon(2, 100.0, 184.0)];
    let mut scene = Scene {
        windows: vec![window(3, rect(100.0, 100.0, 88.0, 96.0))],
        decorations: originals.clone(),
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.decorations[0].bounds.y, 58.0);
    scene.windows.push(window(4, rect(100.0, 58.0, 88.0, 96.0)));
    settle(&mut scene, world());
    assert_eq!(scene.decorations[0].bounds.y, 16.0);
    assert_eq!(scene.decorations[0].home, Some(point(100.0, 100.0)));
    scene.windows.clear();
    settle(&mut scene, world());
    assert_eq!(scene.decorations, originals);
}

#[test]
fn lower_peers_still_block_restoration() {
    let displaced = Decoration {
        home: Some(point(100.0, 100.0)),
        ..icon(2, 400.0, 100.0)
    };
    let mut scene = Scene {
        decorations: vec![icon(1, 100.0, 100.0), displaced],
        ..Scene::default()
    };
    let original = scene.clone();
    restore_background(&mut scene);
    assert_eq!(scene, original);
    settle(&mut scene, world());
    assert_eq!(scene, original);
}

#[test]
fn chained_homes_return_in_one_settlement() {
    let mut scene = Scene {
        decorations: vec![
            Decoration {
                home: Some(point(300.0, 100.0)),
                ..icon(1, 100.0, 100.0)
            },
            Decoration {
                home: Some(point(100.0, 100.0)),
                ..icon(2, 100.0, 58.0)
            },
        ],
        ..Scene::default()
    };
    restore_background(&mut scene);
    assert_eq!(
        scene.decorations,
        [icon(1, 300.0, 100.0), icon(2, 100.0, 100.0)]
    );
}

#[test]
fn decorations_return_together_when_temporary_positions_block_each_others_homes() {
    let mut scene = Scene {
        decorations: vec![
            Decoration {
                home: Some(point(100.0, 100.0)),
                ..icon(1, 100.0, 236.0)
            },
            Decoration {
                home: Some(point(100.0, 204.0)),
                ..icon(2, 100.0, 172.0)
            },
        ],
        ..Scene::default()
    };
    restore_background(&mut scene);
    assert_eq!(
        scene.decorations,
        [icon(1, 100.0, 100.0), icon(2, 100.0, 204.0)]
    );
    let restored = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, restored);
}

#[test]
fn a_stationary_icon_blocks_every_dependent_return_in_a_cycle() {
    let mut scene = Scene {
        decorations: vec![
            Decoration {
                home: Some(point(100.0, 100.0)),
                ..icon(1, 100.0, 236.0)
            },
            Decoration {
                home: Some(point(100.0, 204.0)),
                ..icon(2, 100.0, 172.0)
            },
            icon(3, 100.0, 100.0),
        ],
        ..Scene::default()
    };
    let original = scene.clone();
    restore_background(&mut scene);
    assert_eq!(scene, original);
}

#[test]
fn conflicting_homes_resolve_deterministically_without_overlap_or_oscillation() {
    let lower = Decoration {
        home: Some(point(100.0, 100.0)),
        ..icon(1, 300.0, 100.0)
    };
    let mut scene = Scene {
        decorations: vec![
            lower,
            Decoration {
                home: Some(point(100.0, 100.0)),
                ..icon(2, 500.0, 100.0)
            },
        ],
        ..Scene::default()
    };
    restore_background(&mut scene);
    assert_eq!(scene.decorations, [lower, icon(2, 100.0, 100.0)]);
    let restored = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, restored);
}

#[test]
fn raised_homes_ignore_background_windows_obstacles_and_peers() {
    let home = icon(1, -300.0, -400.0);
    let displaced = Decoration {
        raised: true,
        home: Some(position(home.bounds)),
        ..icon(1, 0.0, 0.0)
    };
    let mut scene = Scene {
        windows: vec![Window {
            background: true,
            ..window(2, home.bounds)
        }],
        obstacles: vec![Obstacle {
            background: true,
            bounds: home.bounds,
        }],
        decorations: vec![displaced, Decoration { id: 3, ..home }],
    };
    restore_background(&mut scene);
    assert_eq!(
        scene.decorations[0],
        Decoration {
            raised: true,
            ..home
        }
    );
}

#[test]
fn ordinary_obstacles_block_the_full_home_including_the_label() {
    let displaced = Decoration {
        home: Some(point(100.0, 100.0)),
        raised: true,
        ..icon(1, 100.0, 58.0)
    };
    let mut scene = Scene {
        decorations: vec![displaced],
        obstacles: vec![Obstacle {
            bounds: rect(100.0, 195.0, 88.0, 20.0),
            background: false,
        }],
        ..Scene::default()
    };
    restore_background(&mut scene);
    assert_eq!(scene.decorations, [displaced]);
}

#[test]
fn restoration_only_does_not_reveal_new_overlaps_or_undo_window_movement() {
    let bounds = rect(100.0, 100.0, 200.0, 200.0);
    let mut scene = Scene {
        windows: vec![window(1, bounds), window(2, bounds)],
        decorations: vec![icon(3, 100.0, 100.0)],
        ..Scene::default()
    };
    let original = scene.clone();
    restore_background(&mut scene);
    assert_eq!(scene, original);
    settle(&mut scene, world());
    scene.windows.pop();
    restore_background(&mut scene);
    assert_eq!(scene.windows[0].bounds, rect(100.0, 76.0, 200.0, 200.0));
}
