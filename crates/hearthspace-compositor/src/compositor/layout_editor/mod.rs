//! Persistent frame attachments with compositor-owned editing and presentation.
//! Native shell surfaces own controls; real application buffers remain ordinary clients.
use super::{
    App,
    windows::{window_app_id, window_title},
};
use hearthspace_common::frame_layout::{
    FrameLayout, LayoutTarget, ObjectPlacement, Point, Rect, TargetKind,
};
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
mod status;
mod surfaces;
mod tooltip;
mod turn;
mod turn_cache;
mod turn_element;
mod turn_input;
mod turn_members;
pub(in crate::compositor) use turn_element::TurnElement;

use hearthspace_ipc::frame_layout::FocusRequest;
pub(super) use hearthspace_ipc::frame_layout::Selection as Target;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlacedTarget {
    pub target: Target,
    pub rect: Rect,
    pub placement: ObjectPlacement,
    pub group: Option<String>,
    pub vertical: bool,
}

#[derive(Default)]
pub(super) struct State {
    // Durable app-slot targets are canonical; placed/session targets are live IDs.
    pub layouts: BTreeMap<String, FrameLayout>,
    pub allow_hide_tools: bool,
    pub revision: u64,
    popup_epoch: u64,
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
    pub(in crate::compositor) fn shell_popup_epoch(&self) -> u64 {
        self.layout_editor.popup_epoch
    }

    /// Status retains invalidation even if the best-effort wakeup is dropped.
    pub(in crate::compositor) fn invalidate_shell_popups(&mut self) {
        self.publish_shell_popup_epoch(true);
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn invalidate_shell_popup_recovery(&mut self) {
        self.publish_shell_popup_epoch(false);
    }

    fn publish_shell_popup_epoch(&mut self, dismiss: bool) {
        self.layout_editor.popup_epoch = self.layout_editor.popup_epoch.wrapping_add(1);
        if let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = socket.set_nonblocking(true);
            let event = serde_json::json!({"version":1,"action":"cancel-shell-popups",
                "popup_epoch":self.layout_editor.popup_epoch,"dismiss":dismiss});
            let _ = socket.send_to(
                &serde_json::to_vec(&event).unwrap(),
                hearthspace_common::shell_event_socket_path(
                    &super::shell_integration::command_socket_path(),
                ),
            );
        }
    }

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
        self.reconcile_canvas_peek();
    }

    pub(in crate::compositor) fn layout_drag_active(&self) -> bool {
        self.layout_editor.drag.is_some()
    }

    pub(in crate::compositor) fn refresh_peek_layout_placements(&mut self) {
        self.layout_editor.placed = self.frame_placements();
        self.layout_changed();
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
        let live = self.live_layout_target(&target.target)?;
        self.windows.iter().enumerate().find_map(|(index, window)| {
            if !window.mapped {
                return None;
            }
            let matches = match target.target.kind {
                TargetKind::Window => {
                    self.is_navigable_window(index) && live.id == window.id.to_string()
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
        let live = self.live_layout_target(&target.target)?;
        self.layout_editor
            .placed
            .iter()
            .find(|item| item.target.output == target.output && item.target.target == live)
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
            .map(|layout| layout.placement(&self.canonical_layout_target(&target.target)))
            .unwrap_or_else(|| hearthspace_common::frame_layout::default_placement(&target.target))
    }
}

fn turns_as_toolbar(target: &LayoutTarget) -> bool {
    target.kind == TargetKind::Group
        || (target.kind == TargetKind::Widget
            && matches!(
                target.id.as_str(),
                "tools" | "apps" | "tray" | "clock" | "notifications"
            ))
}
