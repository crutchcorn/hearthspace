use super::{tests::*, *};

#[test]
fn icon_artwork_is_centered_above_the_label() {
    assert_eq!(
        icon_body(rect(100.0, 100.0, 88.0, 96.0)),
        rect(116.0, 110.0, 56.0, 56.0)
    );
    assert_eq!(
        icon_body(rect(-30.0, -40.0, 20.0, 12.0)),
        rect(-26.0, -40.0, 12.0, 12.0)
    );
}

#[test]
fn exposed_transparent_padding_does_not_prevent_artwork_from_peeking() {
    let item = icon(1, 100.0, 100.0);
    let cover = window(2, item.body);
    let mut scene = Scene {
        windows: vec![cover],
        decorations: vec![item],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(position(scene.decorations[0].bounds), point(100.0, 68.0));
    assert_eq!(scene.decorations[0].body, rect(116.0, 78.0, 56.0, 56.0));
    assert_eq!(scene.decorations[0].home, Some(point(100.0, 100.0)));
    assert_eq!(scene.windows, [cover]);
}

#[test]
fn a_window_covering_the_button_reveals_artwork_rather_than_padding() {
    let item = icon(1, 100.0, 100.0);
    let mut scene = Scene {
        windows: vec![window(2, item.bounds)],
        decorations: vec![item],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(position(scene.decorations[0].bounds), point(100.0, 58.0));
    let settled = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, settled);
}

#[test]
fn higher_icon_artwork_displaces_a_lower_icon() {
    let lower = icon(1, 100.0, 100.0);
    let upper = icon(2, 100.0, 100.0);
    let mut scene = Scene {
        decorations: vec![lower, upper],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(position(scene.decorations[0].bounds), point(100.0, 68.0));
    assert_eq!(scene.decorations[1], upper);
    let settled = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, settled);
}

#[test]
fn transparent_padding_does_not_occlude_another_icon() {
    let mut scene = Scene {
        decorations: vec![icon(1, 100.0, 100.0), icon(2, 100.0, 156.0)],
        ..Scene::default()
    };
    let original = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, original);
}

#[test]
fn a_partly_visible_icon_body_stays_put_without_acquiring_a_home() {
    let item = icon(1, 100.0, 100.0);
    let cover = window(
        2,
        Rect {
            y: item.body.y + 0.001,
            ..item.body
        },
    );
    let mut scene = Scene {
        windows: vec![cover],
        decorations: vec![item],
        ..Scene::default()
    };
    let original = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, original);
}

#[test]
fn controls_use_the_entire_rectangle_as_their_body() {
    let cover = window(1, rect(100.0, 100.0, 200.0, 200.0));
    let battery = control(2, rect(140.0, 140.0, 54.0, 54.0), true);
    let mut scene = Scene {
        windows: vec![cover],
        decorations: vec![battery],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.decorations[0].bounds, rect(140.0, 68.0, 54.0, 54.0));
    assert_eq!(scene.decorations[0].body, scene.decorations[0].bounds);
    assert_eq!(scene.decorations[0].home, Some(point(140.0, 140.0)));
    scene.windows.clear();
    settle(&mut scene, world());
    assert_eq!(scene.decorations, [battery]);
}

#[test]
fn a_wide_control_with_an_exposed_outer_edge_does_not_move() {
    let mut scene = Scene {
        windows: vec![window(1, rect(100.0, 100.0, 200.0, 200.0))],
        decorations: vec![control(2, rect(140.0, 140.0, 240.0, 54.0), true)],
        ..Scene::default()
    };
    let original = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, original);
}

#[test]
fn raised_decorations_settle_before_background_peers_regardless_of_array_order() {
    let raised = Decoration {
        raised: true,
        ..icon(1, 100.0, 100.0)
    };
    let lower = icon(2, 100.0, 100.0);
    let mut scene = Scene {
        decorations: vec![raised, lower],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.decorations[0], raised);
    assert_eq!(position(scene.decorations[1].bounds), point(100.0, 68.0));
}

#[test]
fn background_windows_and_obstacles_do_not_cover_raised_controls() {
    let bounds = rect(100.0, 100.0, 200.0, 200.0);
    let raised = control(1, rect(140.0, 140.0, 54.0, 54.0), true);
    let lower = Decoration {
        id: 2,
        raised: false,
        ..raised
    };
    let cover = Window {
        background: true,
        ..window(3, bounds)
    };
    let mut scene = Scene {
        windows: vec![cover],
        decorations: vec![raised, lower],
        obstacles: vec![Obstacle {
            bounds,
            background: true,
        }],
    };
    settle_background(&mut scene, world());
    assert_eq!(scene.decorations[0], raised);
    assert_eq!(position(scene.decorations[1].bounds), point(140.0, 68.0));
    assert_eq!(scene.windows, [cover]);
}

#[test]
fn ordinary_placement_obstacles_cover_raised_decorations() {
    let item = Decoration {
        raised: true,
        ..icon(1, 100.0, 100.0)
    };
    let mut scene = Scene {
        decorations: vec![item],
        obstacles: vec![Obstacle {
            bounds: item.bounds,
            background: false,
        }],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(position(scene.decorations[0].bounds), point(100.0, 58.0));
}

#[test]
fn background_only_settlement_preserves_overlapping_windows() {
    let bounds = rect(100.0, 100.0, 200.0, 200.0);
    let windows = vec![window(1, bounds), window(2, bounds)];
    let mut scene = Scene {
        windows: windows.clone(),
        decorations: vec![icon(3, 100.0, 100.0)],
        ..Scene::default()
    };
    settle_background(&mut scene, world());
    assert_eq!(scene.windows, windows);
    assert_eq!(scene.decorations[0].home, Some(point(100.0, 100.0)));
}

#[test]
fn impossible_decoration_placements_do_not_acquire_homes() {
    let item = icon(1, 100.0, 100.0);
    let mut scene = Scene {
        decorations: vec![item],
        windows: vec![window(2, world().unwrap())],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.decorations, [item]);
}
