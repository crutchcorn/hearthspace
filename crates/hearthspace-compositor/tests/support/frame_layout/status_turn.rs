//! Standalone status surfaces reflow after the drag preview changes orientation.
//! MapClient supplies real Wayland buffers; shell label rendering is covered separately.
use super::super::canvas_map_client::MapClient;
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameLayout, LayoutTarget, ObjectPlacement, WIDGETS, frame_widget_app_id,
};

struct Fixture {
    compositor: HeadlessCompositor,
    client: MapClient,
    surface: usize,
    id: &'static str,
    _saved: tempfile::TempDir,
}

impl Fixture {
    fn start(id: &'static str, width: i32, expanded: bool, motion: bool) -> Self {
        let saved = tempfile::tempdir().unwrap();
        let path = saved.path().join("layout.json");
        let mut model = FrameLayout {
            allow_hide_tools: true,
            ..Default::default()
        };
        // Only this control participates: another toolbar cannot keep animating for it.
        for widget in WIDGETS {
            model.set_placement(&LayoutTarget::widget(*widget), ObjectPlacement::Hidden);
        }
        model.set_placement(
            &LayoutTarget::widget(id),
            ObjectPlacement::frame(FrameEdge::Top, 0.5),
        );
        model.expanded.insert(id.into(), expanded);
        model.validate().unwrap();
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version":1,"x":0,"y":0,"scale":1.0,"placements":[],
                "frames":{"hearthspace-0":model},"allow_hide_tools":true,
            }))
            .unwrap(),
        )
        .unwrap();
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        action(&compositor, format!("canvas-search-motion {motion}"));
        let output = layout(&compositor)["outputs"][0]["output"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut client = connect(&compositor);
        let surface = client.add(
            frame_widget_app_id(Some(&output), id),
            (width, 54),
            (0, 0),
            COLOR,
        );
        action(&compositor, "layout-tool");
        Self {
            compositor,
            client,
            surface,
            id,
            _saved: saved,
        }
    }

    fn widget<'a>(&self, state: &'a Value) -> &'a Value {
        state["outputs"][0]["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|widget| widget["id"] == self.id)
            .unwrap()
    }

    fn shape(&self, vertical: bool, size: (i32, i32)) {
        let state = layout(&self.compositor);
        let widget = self.widget(&state);
        assert_eq!(widget["group"], Value::Null, "{state}");
        assert_eq!(
            widget["orientation"],
            if vertical { "vertical" } else { "horizontal" },
            "{} drag preview did not turn: {state}",
            self.id
        );
        assert_eq!(widget["rect"]["width"], f64::from(size.0), "{state}");
        assert_eq!(widget["rect"]["height"], f64::from(size.1), "{state}");
    }

    fn placement(&self) -> Value {
        layout(&self.compositor)["outputs"][0]["layout"]["placements"]
            [LayoutTarget::widget(self.id).key()]
        .clone()
    }

    fn move_to(&self, x: f64, y: f64) {
        action(&self.compositor, format!("pointer-motion-abs {x} {y}"));
    }

    fn point_at_control(&self) {
        let state = layout(&self.compositor);
        let rect = &self.widget(&state)["rect"];
        self.move_to(
            rect["x"].as_f64().unwrap() + 16.,
            rect["y"].as_f64().unwrap() + 20.,
        );
    }

    fn pickup(&self) {
        self.point_at_control();
        action(&self.compositor, "pointer-button-down 272");
        assert_eq!(
            layout(&self.compositor)["selected"]["target"],
            json!({"kind":"widget","id":self.id})
        );
    }

    fn release(&self) {
        action(&self.compositor, "pointer-button-up 272");
    }

    fn pending_reflow(&mut self, old_size: (i32, i32), vertical: bool) {
        self.shape(vertical, (old_size.1, old_size.0));
        assert_eq!(self.client.size(self.surface), old_size);
        // Outlast both the 280ms turn and the guide fade. The old buffer must
        // keep the turn pending, not consume its animation before native reflow.
        thread::sleep(Duration::from_millis(350));
        let state = layout(&self.compositor);
        assert_eq!(
            state["animating"], true,
            "{} turn expired before native reflow: {state}",
            self.id
        );
    }

    fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let state = layout(&self.compositor);
            if state["animating"] == false {
                break;
            }
            assert!(Instant::now() < deadline, "turn did not retire: {state}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn reflow_turn(&mut self, size: (i32, i32), vertical: bool) {
        self.client.resize(self.surface, size);
        assert_eq!(
            layout(&self.compositor)["animating"],
            true,
            "native reflow must start the pending turn's animation"
        );
        self.settle();
        assert_eq!(self.client.size(self.surface), size);
        self.shape(vertical, size);
    }

    fn corner(&self, x: f64, y: f64, edge: FrameEdge, position: f64) {
        self.move_to(x, y);
        self.release();
        assert_eq!(
            self.placement(),
            serde_json::to_value(ObjectPlacement::frame(edge, position)).unwrap()
        );
        self.settle();
    }

    fn quit(&mut self) {
        assert!(self.client.closed().is_empty());
        action(&self.compositor, "quit");
        wait_for_exit(&mut self.compositor.child);
    }
}

fn expanded_turns_both_ways(id: &'static str, width: i32, reflowed_length: i32) {
    let mut fixture = Fixture::start(id, width, true, true);
    fixture.shape(false, (width, 54));
    assert_eq!(
        fixture.widget(&layout(&fixture.compositor))["expanded"],
        true
    );
    fixture.pickup();
    fixture.move_to(20., 400.);
    fixture.pending_reflow((width, 54), true);
    // A changed status label may also change its length when the shell reflows.
    fixture.reflow_turn((54, reflowed_length), true);
    fixture.corner(45., 755., FrameEdge::Left, 1.);
    fixture.shape(true, (54, reflowed_length));

    fixture.pickup();
    fixture.move_to(640., 20.);
    fixture.pending_reflow((54, reflowed_length), false);
    fixture.reflow_turn((reflowed_length, 54), false);
    fixture.corner(1235., 45., FrameEdge::Top, 1.);
    fixture.shape(false, (reflowed_length, 54));
    assert!(fixture.client.presses().is_empty());
    assert!(fixture.client.releases().is_empty());
    fixture.quit();
}

#[test]
fn expanded_notifications_turn_waits_for_native_reflow_in_both_directions() {
    let _guard = headless_test_guard();
    expanded_turns_both_ways("notifications", 132, 168);
}

#[test]
fn expanded_clock_turn_waits_for_native_reflow_in_both_directions() {
    let _guard = headless_test_guard();
    expanded_turns_both_ways("clock", 104, 104);
}

#[test]
fn expanded_notifications_pending_turn_cancels_and_canvas_drop_returns_to_a_row() {
    let _guard = headless_test_guard();
    let mut fixture = Fixture::start("notifications", 132, true, true);
    let original = layout(&fixture.compositor)["outputs"][0]["layout"].clone();
    fixture.pickup();
    fixture.move_to(20., 400.);
    fixture.pending_reflow((132, 54), true);
    keys(&fixture.compositor, &[1]);
    fixture.release();
    let cancelled = layout(&fixture.compositor);
    assert_eq!(cancelled["editing"], false);
    assert_eq!(cancelled["animating"], false);
    assert_eq!(cancelled["outputs"][0]["layout"], original);
    fixture.shape(false, (132, 54));

    action(&fixture.compositor, "layout-tool");
    fixture.pickup();
    fixture.move_to(20., 400.);
    fixture.pending_reflow((132, 54), true);
    fixture.reflow_turn((54, 132), true);
    fixture.move_to(640., 400.);
    fixture.shape(true, (54, 132));
    fixture.release();
    assert_eq!(fixture.placement()["surface"], "canvas");
    fixture.pending_reflow((54, 132), false);
    fixture.reflow_turn((132, 54), false);
    assert_eq!(fixture.placement()["surface"], "canvas");
    assert!(fixture.client.presses().is_empty());
    assert!(fixture.client.releases().is_empty());
    fixture.quit();
}

#[test]
fn expanded_status_reduced_motion_changes_orientation_without_animation() {
    let _guard = headless_test_guard();
    for (id, width) in [("notifications", 132), ("clock", 104)] {
        let mut fixture = Fixture::start(id, width, true, false);
        fixture.pickup();
        fixture.move_to(20., 400.);
        fixture.shape(true, (54, width));
        assert_eq!(layout(&fixture.compositor)["animating"], false);
        assert_eq!(fixture.client.size(fixture.surface), (width, 54));
        fixture.client.resize(fixture.surface, (54, width));
        fixture.corner(45., 755., FrameEdge::Left, 1.);
        fixture.shape(true, (54, width));

        fixture.pickup();
        fixture.move_to(640., 20.);
        fixture.shape(false, (width, 54));
        assert_eq!(layout(&fixture.compositor)["animating"], false);
        fixture.client.resize(fixture.surface, (width, 54));
        fixture.corner(1235., 45., FrameEdge::Top, 1.);
        fixture.shape(false, (width, 54));
        assert_eq!(layout(&fixture.compositor)["animating"], false);
        assert!(fixture.client.presses().is_empty());
        assert!(fixture.client.releases().is_empty());
        fixture.quit();
    }
}

#[test]
fn collapsed_status_controls_stay_square_and_receive_clicks_after_layout_editing() {
    let _guard = headless_test_guard();
    for id in ["notifications", "clock"] {
        let mut fixture = Fixture::start(id, 54, false, true);
        // Square buffers need no resize. This checks shape/input, not icon rotation.
        for (x, y, edge) in [(20., 400., "left"), (640., 20., "top")] {
            fixture.pickup();
            fixture.move_to(x, y);
            let dragging = layout(&fixture.compositor);
            fixture.release();
            fixture.settle();
            let dropped = layout(&fixture.compositor);
            for state in [&dragging, &dropped] {
                let widget = fixture.widget(state);
                assert_eq!(widget["rect"]["width"], 54., "{state}");
                assert_eq!(widget["rect"]["height"], 54., "{state}");
                assert_eq!(widget["expanded"], false, "{state}");
            }
            assert_eq!(fixture.placement()["edge"], edge);
        }
        assert_eq!(fixture.client.size(fixture.surface), (54, 54));
        assert!(fixture.client.presses().is_empty());
        assert!(fixture.client.releases().is_empty());
        command(&fixture.compositor, "done", Value::Null, Value::Null);
        fixture.point_at_control();
        action(&fixture.compositor, "pointer-button-down 272");
        fixture.release();
        assert_eq!(fixture.client.presses().len(), 1);
        assert_eq!(fixture.client.presses()[0].0, fixture.surface);
        assert_eq!(fixture.client.releases().len(), 1);
        assert_eq!(fixture.client.releases()[0].0, fixture.surface);
        fixture.quit();
    }
}
