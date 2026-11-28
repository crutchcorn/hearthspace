//! Single-output fixtures discover opaque instance IDs by capability.
use super::{HeadlessCompositor, navigation::query};
use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget, starter_placement};
use serde_json::{Value, json};

pub(super) fn id(compositor: &HeadlessCompositor, kind: &str) -> String {
    let state = query(compositor, "layout-status");
    state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["kind"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} fixture widget: {state}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

pub(super) fn app_id(compositor: &HeadlessCompositor, output: Option<&str>, kind: &str) -> String {
    let identity = id(compositor, kind);
    // Some tests deliberately declare an unknown output with a valid widget ID.
    hearthspace_common::frame_layout::frame_widget_app_id(output, &identity)
}

pub(super) fn target(compositor: &HeadlessCompositor, kind: &str) -> Value {
    json!({"kind":"widget","id":id(compositor, kind)})
}

pub(super) fn model_target(model: &FrameLayout, kind: &str) -> LayoutTarget {
    LayoutTarget::widget(model.widget_id(kind).expect("fixture widget"))
}

pub(super) fn only(kinds: &[&str]) -> FrameLayout {
    let mut model = FrameLayout::empty();
    model.allow_hide_tools = true;
    for kind in kinds {
        model.add_widget(kind, starter_placement(kind)).unwrap();
    }
    model
}
