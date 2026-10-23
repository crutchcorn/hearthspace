//! Persistent frame attachments with compositor-owned editing and presentation.
//! Native shell surfaces own controls; real application buffers remain ordinary clients.
use super::{
    App,
    windows::{window_app_id, window_title},
};
use hearthspace_common::frame_layout::{
    FrameLayout, LayoutTarget, ObjectPlacement, Point, Rect, TargetKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

mod actions;
mod availability;
mod compact;
mod confirmation;
mod emphasis;
mod geometry;
mod grip;
mod input;
mod motion;
mod overflow;
mod overlays;
mod placement;
mod projection;
mod protocol;
mod render;
mod render_turn;
mod settlement;
mod surfaces;
mod tooltip;
mod turn;
mod turn_cache;
mod turn_element;
mod turn_input;
mod turn_members;
pub(in crate::compositor) use turn_element::TurnElement;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Target {
    pub output: String,
    pub target: LayoutTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlacedTarget {
    pub target: Target,
    pub rect: Rect,
    pub placement: ObjectPlacement,
    pub group: Option<String>,
    pub vertical: bool,
}

#[derive(Clone, Serialize)]
struct FocusRequest {
    serial: u64,
    output: String,
    target: LayoutTarget,
    reverse: bool,
}

#[derive(Default)]
pub(super) struct State {
    pub layouts: BTreeMap<String, FrameLayout>,
    pub allow_hide_tools: bool,
    pub revision: u64,
    sequence: u64,
    session: Option<Session>,
    pub placed: Vec<PlacedTarget>,
    popup_anchors: HashMap<String, Target>,
    focus_request: Option<FocusRequest>,
    focus_serial: u64,
    pub expanded_window: Option<u64>,
    pub hovered_window: Option<u64>,
    pub overlays: overlays::OverlayCache,
    guides: render::GuideMotion,
    motion: motion::State,
    turn: turn::State,
    drag: Option<input::Drag>,
    pub(in crate::compositor) suppress_release: bool,
    consumed_buttons: std::collections::BTreeSet<u32>,
    unavailable: std::collections::BTreeSet<(String, String)>,
    pressed_item: Option<String>,
    drop_anchor: Option<(Point, Rect, bool)>,
    emphasis: emphasis::State,
    overflow: overflow::State,
}

struct Session {
    started: std::time::Instant,
    id: u64,
    editing: bool,
    output: String,
    selected: Option<Target>,
    previous_focus: Option<u64>,
    confirmation: Option<confirmation::Confirmation>,
    confirmation_focus: Option<u64>,
}

impl State {
    pub(in crate::compositor) fn from_layouts(
        layouts: BTreeMap<String, FrameLayout>,
        allow_hide_tools: bool,
    ) -> Self {
        Self {
            layouts,
            allow_hide_tools,
            ..Self::default()
        }
    }
}
impl App {
    pub(in crate::compositor) fn layout_editing(&self) -> bool {
        self.layout_editor
            .session
            .as_ref()
            .is_some_and(|session| session.editing)
    }

    pub(in crate::compositor) fn reconcile_layout_editor(&mut self) {
        self.advance_layout_motion();
        self.advance_layout_turn();
        self.advance_layout_guides();
        if self.session_locked() {
            self.update_layout_emphasis();
            return;
        }
        self.reconcile_layout_confirmation();
        for output in self.capture_outputs() {
            let allow = self.layout_editor.allow_hide_tools;
            self.layout_editor
                .layouts
                .entry(output.name)
                .or_insert_with(|| FrameLayout {
                    allow_hide_tools: allow,
                    ..Default::default()
                });
        }
        let next = self.frame_placements();
        if next != self.layout_editor.placed {
            self.layout_editor.placed = next;
            if self.settle_frame_overlaps() {
                self.layout_editor.placed = self.frame_placements();
            }
            self.layout_changed();
        }
        self.update_layout_emphasis();
    }

    pub(in crate::compositor) fn layout_changed(&mut self) {
        self.layout_editor.revision = self.layout_editor.revision.wrapping_add(1);
        if let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = socket.set_nonblocking(true);
            let _ = socket.send_to(
                br#"{"version":1,"action":"layout-changed"}"#,
                hearthspace_common::shell_event_socket_path(
                    &super::shell_integration::command_socket_path(),
                ),
            );
        }
        self.request_redraw();
    }

    fn target_window_index(&self, target: &Target) -> Option<usize> {
        self.windows.iter().enumerate().find_map(|(index, window)| {
            if !window.mapped {
                return None;
            }
            let matches = match target.target.kind {
                TargetKind::Window => {
                    self.is_navigable_window(index) && target.target.id == window.id.to_string()
                }
                TargetKind::Item => {
                    self.desktop_id(index).as_deref() == Some(target.target.id.as_str())
                }
                TargetKind::Widget => self
                    .frame_widget_identity(index)
                    .is_some_and(|(output, id)| output == target.output && id == target.target.id),
                TargetKind::Group => self
                    .frame_group_identity(index)
                    .is_some_and(|(output, id)| output == target.output && id == target.target.id),
            };
            matches.then_some(index)
        })
    }

    fn target_title(&self, target: &Target) -> String {
        match target.target.kind {
            TargetKind::Widget => {
                hearthspace_common::frame_layout::widget_title(&target.target.id).into()
            }
            TargetKind::Group => "Frame group".into(),
            _ => self
                .target_window_index(target)
                .and_then(|i| window_title(&self.windows[i].surface))
                .unwrap_or_else(|| target.target.id.clone())
                .chars()
                .take(4096)
                .collect(),
        }
    }

    fn frame_placed(&self, target: &Target) -> Option<&PlacedTarget> {
        self.layout_editor
            .placed
            .iter()
            .find(|item| item.target == *target)
    }

    fn target_placement(&self, target: &Target) -> ObjectPlacement {
        if target.target.kind == TargetKind::Group && compact::backing(&target.target.id) {
            return self
                .frame_placed(target)
                .map_or(ObjectPlacement::Hidden, |p| p.placement);
        }
        self.layout_editor
            .layouts
            .get(&target.output)
            .map(|layout| layout.placement(&target.target))
            .unwrap_or_else(|| hearthspace_common::frame_layout::default_placement(&target.target))
    }
}

fn turns_as_toolbar(target: &LayoutTarget) -> bool {
    target.kind == TargetKind::Group
        || (target.kind == TargetKind::Widget
            && matches!(target.id.as_str(), "tools" | "apps" | "tray"))
}
