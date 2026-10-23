//! Real pointer drags preserve the preview's axis when committing a corner drop.
use super::super::canvas_map_client::MapClient;
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind, WIDGETS,
    frame_group_app_id, frame_group_grip_app_id, frame_widget_app_id,
};
use std::path::Path;

const GROUP: &str = "corner-tools";

fn seed(path: &Path, grouped: bool) {
    let mut model = FrameLayout {
        allow_hide_tools: true,
        ..Default::default()
    };
    for id in WIDGETS {
        model.set_placement(&LayoutTarget::widget(*id), ObjectPlacement::Hidden);
    }
    let target = if grouped {
        let members: Vec<_> = ["tools", "tray"]
            .into_iter()
            .map(LayoutTarget::widget)
            .collect();
        for member in &members {
            model.placements.remove(&member.key());
        }
        model.groups.insert(
            GROUP.into(),
            FrameGroup {
                id: GROUP.into(),
                members,
            },
        );
        LayoutTarget::new(TargetKind::Group, GROUP)
    } else {
        LayoutTarget::widget("tools")
    };
    model.set_placement(&target, ObjectPlacement::frame(FrameEdge::Top, 0.5));
    model.validate().unwrap();
    std::fs::write(
        path,
        serde_json::to_vec(&json!({
            "version":1,"x":0,"y":0,"scale":1.0,"placements":[],
            "frames":{"hearthspace-0":model},"allow_hide_tools":true,
        }))
        .unwrap(),
    )
    .unwrap();
}

struct Fixture {
    compositor: HeadlessCompositor,
    client: MapClient,
    target: LayoutTarget,
    native: Vec<(usize, (i32, i32))>,
}

impl Fixture {
    fn start(path: &Path, grouped: bool) -> Self {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(path));
        drop(compositor.wait_for_socket());
        action(&compositor, "canvas-search-motion false");
        let output = layout(&compositor)["outputs"][0]["output"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut client = connect(&compositor);
        let size = if grouped { (132, 44) } else { (194, 54) };
        let tools = client.add(
            frame_widget_app_id(Some(&output), "tools"),
            size,
            (0, 0),
            COLOR,
        );
        let mut native = vec![(tools, size)];
        let target = if grouped {
            for (id, size) in [
                (frame_widget_app_id(Some(&output), "tray"), (88, 44)),
                (frame_group_app_id(Some(&output), GROUP), (234, 54)),
                (frame_group_grip_app_id(Some(&output), GROUP), (28, 24)),
            ] {
                native.push((client.add(id, size, (0, 0), COLOR), size));
            }
            LayoutTarget::new(TargetKind::Group, GROUP)
        } else {
            LayoutTarget::widget("tools")
        };
        action(&compositor, "layout-tool");
        Self {
            compositor,
            client,
            target,
            native,
        }
    }

    fn projected<'a>(&self, state: &'a Value) -> &'a Value {
        let collection = if self.target.kind == TargetKind::Group {
            "groups"
        } else {
            "widgets"
        };
        state["outputs"][0][collection]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["id"] == self.target.id)
            .unwrap()
    }

    fn reflow(&mut self, vertical: bool) {
        for &(surface, (width, height)) in &self.native {
            self.client.resize(
                surface,
                if vertical {
                    (height, width)
                } else {
                    (width, height)
                },
            );
        }
    }

    fn assert_axis(&self, vertical: bool) {
        let state = layout(&self.compositor);
        let orientation = if vertical { "vertical" } else { "horizontal" };
        let projected = self.projected(&state);
        assert_eq!(projected["orientation"], orientation, "{state}");
        let rect = &projected["rect"];
        assert_eq!(
            rect["height"].as_f64().unwrap() > rect["width"].as_f64().unwrap(),
            vertical,
            "{state}"
        );
        if self.target.kind == TargetKind::Group {
            for id in ["tools", "tray"] {
                let member = state["outputs"][0]["widgets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|widget| widget["id"] == id)
                    .unwrap();
                assert_eq!(member["group"], GROUP, "{state}");
                assert_eq!(member["orientation"], orientation, "{state}");
            }
        }
    }

    fn assert_placement(&self, edge: FrameEdge, position: f64) {
        let state = layout(&self.compositor);
        assert_eq!(
            state["outputs"][0]["layout"]["placements"][self.target.key()],
            serde_json::to_value(ObjectPlacement::frame(edge, position)).unwrap(),
            "{state}"
        );
        self.assert_axis(edge.is_vertical());
    }

    fn place(&mut self, edge: FrameEdge) {
        command(
            &self.compositor,
            "place",
            serde_json::to_value(&self.target).unwrap(),
            serde_json::to_value(ObjectPlacement::frame(edge, 0.5)).unwrap(),
        );
        self.reflow(edge.is_vertical());
        self.assert_placement(edge, 0.5);
    }

    fn pickup(&self) {
        let state = layout(&self.compositor);
        let projected = self.projected(&state);
        let (rect, dx, dy) = if self.target.kind == TargetKind::Group {
            let rect = &projected["grip"];
            (
                rect,
                rect["width"].as_f64().unwrap() / 2.,
                rect["height"].as_f64().unwrap() / 2.,
            )
        } else {
            // The final attachment must follow this off-center pointer grab.
            (&projected["rect"], 16., 20.)
        };
        self.move_to(
            rect["x"].as_f64().unwrap() + dx,
            rect["y"].as_f64().unwrap() + dy,
        );
        action(&self.compositor, "pointer-button-down 272");
        assert_eq!(
            layout(&self.compositor)["selected"]["target"],
            serde_json::to_value(&self.target).unwrap()
        );
    }

    fn move_to(&self, x: f64, y: f64) {
        action(&self.compositor, format!("pointer-motion-abs {x} {y}"));
    }

    fn approach(&mut self, edge: FrameEdge) {
        let (x, y) = match edge {
            FrameEdge::Top => (640., 20.),
            FrameEdge::Bottom => (640., 780.),
            FrameEdge::Left => (20., 400.),
            FrameEdge::Right => (1260., 400.),
        };
        self.move_to(x, y);
        self.reflow(edge.is_vertical());
        self.assert_axis(edge.is_vertical());
    }

    fn release(&self) {
        action(&self.compositor, "pointer-button-up 272");
    }

    fn quit(&mut self) {
        assert!(self.client.presses().is_empty());
        assert!(self.client.releases().is_empty());
        action(&self.compositor, "quit");
        wait_for_exit(&mut self.compositor.child);
    }
}

fn corner_drop_scenarios(grouped: bool) {
    use FrameEdge::{Bottom, Left, Right, Top};
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    seed(&path, grouped);
    let mut fixture = Fixture::start(&path, grouped);
    for (x, y, horizontal, vertical, h_position, v_position) in [
        (45., 45., Top, Left, 0., 0.),
        (1235., 45., Top, Right, 1., 0.),
        (45., 755., Bottom, Left, 0., 1.),
        (1235., 755., Bottom, Right, 1., 1.),
    ] {
        for (edge, position) in [(horizontal, h_position), (vertical, v_position)] {
            fixture.place(if edge.is_vertical() { Bottom } else { Left });
            fixture.pickup();
            // Multiple turns establish that the last approach wins over the
            // original placement and any intermediate edge along the route.
            fixture.approach(Top);
            fixture.approach(Left);
            fixture.approach(edge);
            fixture.move_to(640., 400.);
            fixture.assert_axis(edge.is_vertical());
            fixture.move_to(x, y);
            fixture.assert_axis(edge.is_vertical());
            fixture.release();
            fixture.assert_placement(edge, position);
        }
    }
    fixture.quit();
}

#[test]
fn frame_layout_toolbar_corner_drop_keeps_latest_drag_orientation_at_every_corner() {
    let _guard = headless_test_guard();
    corner_drop_scenarios(false);
}

#[test]
fn frame_layout_group_corner_drop_keeps_latest_drag_orientation_at_every_corner() {
    let _guard = headless_test_guard();
    corner_drop_scenarios(true);
}

#[test]
fn frame_layout_corner_cancel_restores_original_axis_and_reload_keeps_committed_axis() {
    let _guard = headless_test_guard();
    for grouped in [false, true] {
        let saved = tempfile::tempdir().unwrap();
        let path = saved.path().join("layout.json");
        seed(&path, grouped);
        {
            let mut fixture = Fixture::start(&path, grouped);
            let before = layout(&fixture.compositor)["outputs"][0]["layout"].clone();
            fixture.pickup();
            fixture.approach(FrameEdge::Left);
            fixture.move_to(45., 755.);
            fixture.assert_axis(true);
            keys(&fixture.compositor, &[1]);
            fixture.release();
            let cancelled = layout(&fixture.compositor);
            assert_eq!(cancelled["editing"], false);
            assert_eq!(cancelled["outputs"][0]["layout"], before);
            fixture.assert_placement(FrameEdge::Top, 0.5);
            fixture.reflow(false);
            action(&fixture.compositor, "layout-tool");
            fixture.pickup();
            fixture.approach(FrameEdge::Left);
            fixture.move_to(45., 755.);
            fixture.release();
            fixture.assert_placement(FrameEdge::Left, 1.);
            command(&fixture.compositor, "done", Value::Null, Value::Null);
            fixture.quit();
        }
        let mut reloaded = Fixture::start(&path, grouped);
        reloaded.assert_placement(FrameEdge::Left, 1.);
        reloaded.quit();
    }
}

#[test]
fn frame_layout_interrupted_turn_corner_drop_keeps_the_captured_axis() {
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    seed(&path, false);
    let mut fixture = Fixture::start(&path, false);
    for (displayed, pending, x, y, expected) in [
        (FrameEdge::Top, FrameEdge::Left, 1235., 45., FrameEdge::Top),
        (FrameEdge::Left, FrameEdge::Top, 45., 755., FrameEdge::Left),
    ] {
        action(&fixture.compositor, "canvas-search-motion false");
        fixture.place(displayed);
        action(&fixture.compositor, "canvas-search-motion true");
        let old = fixture.projected(&layout(&fixture.compositor))["rect"].clone();
        // Cache the visible composition, then leave the new layout awaiting
        // native reflow so pickup must use the old pixels and their orientation.
        let _ = fixture.compositor.take_screenshot();
        command(
            &fixture.compositor,
            "place",
            serde_json::to_value(&fixture.target).unwrap(),
            serde_json::to_value(ObjectPlacement::frame(pending, 0.5)).unwrap(),
        );
        assert_eq!(layout(&fixture.compositor)["animating"], true);
        fixture.move_to(
            old["x"].as_f64().unwrap() + 16.,
            old["y"].as_f64().unwrap() + 20.,
        );
        action(&fixture.compositor, "pointer-button-down 272");
        fixture.assert_axis(displayed.is_vertical());
        fixture.move_to(640., 400.);
        fixture.assert_axis(displayed.is_vertical());
        fixture.move_to(x, y);
        fixture.assert_axis(displayed.is_vertical());
        fixture.release();
        fixture.assert_placement(expected, 1.);
    }
    fixture.quit();
}
