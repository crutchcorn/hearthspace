use smithay::{
    desktop::{
        PopupManager, WindowSurfaceType,
        utils::{bbox_from_surface_tree, under_from_surface_tree},
    },
    reexports::winit::window::CursorIcon,
    utils::{Logical, Physical, Point, Rectangle, SERIAL_COUNTER, Size},
    wayland::{
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData},
    },
};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    shell::server::xdg_toplevel,
};
use wayland_server::protocol::wl_surface::{self, WlSurface};

use crate::{
    accessibility::ManagedWindowAccessibilityInfo,
    config::*,
    geometry::{CanvasPoint, rect_contains},
};

use super::{
    App, HitTarget, ManagedWindow, ManagedWindowKind, ResizeState, WindowDecoration,
    idle::ActivityReason, rendering::toplevel_geometry_loc,
};

/// Which edges of a window are being dragged during an interactive resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct ResizeEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl ResizeEdges {
    pub(super) fn is_empty(self) -> bool {
        !(self.left || self.right || self.top || self.bottom)
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdges {
    fn from(edges: xdg_toplevel::ResizeEdge) -> Self {
        use xdg_toplevel::ResizeEdge;
        match edges {
            ResizeEdge::Top => Self {
                top: true,
                ..Self::default()
            },
            ResizeEdge::Bottom => Self {
                bottom: true,
                ..Self::default()
            },
            ResizeEdge::Left => Self {
                left: true,
                ..Self::default()
            },
            ResizeEdge::Right => Self {
                right: true,
                ..Self::default()
            },
            ResizeEdge::TopLeft => Self {
                top: true,
                left: true,
                ..Self::default()
            },
            ResizeEdge::TopRight => Self {
                top: true,
                right: true,
                ..Self::default()
            },
            ResizeEdge::BottomLeft => Self {
                bottom: true,
                left: true,
                ..Self::default()
            },
            ResizeEdge::BottomRight => Self {
                bottom: true,
                right: true,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

fn configure_server_side_decoration(toplevel: &ToplevelSurface) {
    if window_kind_for_toplevel(toplevel) == ManagedWindowKind::ShellBar {
        return;
    }

    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    toplevel.send_configure();
}

fn configure_client_side_decoration(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ClientSide);
    });
    toplevel.send_configure();
}

pub(super) fn window_kind_for_toplevel(surface: &ToplevelSurface) -> ManagedWindowKind {
    match toplevel_app_id(surface).as_deref() {
        Some(SHELL_BAR_APP_ID) => ManagedWindowKind::ShellBar,
        _ => ManagedWindowKind::Normal,
    }
}

fn toplevel_app_id(surface: &ToplevelSurface) -> Option<String> {
    with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok()?.app_id.clone())
    })
}

pub(super) fn toplevel_title(surface: &ToplevelSurface) -> Option<String> {
    with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok()?.title.clone())
    })
}

pub(super) fn position_for_new_window(
    kind: ManagedWindowKind,
    fallback: CanvasPoint,
) -> CanvasPoint {
    match kind {
        ManagedWindowKind::Normal => fallback,
        ManagedWindowKind::ShellBar => CanvasPoint { x: 0, y: 0 },
    }
}

pub(super) fn decoration_for_new_window(kind: ManagedWindowKind) -> WindowDecoration {
    match kind {
        ManagedWindowKind::Normal => WindowDecoration::ClientSide,
        ManagedWindowKind::ShellBar => WindowDecoration::ClientSide,
    }
}

impl App {
    pub(super) fn set_keyboard_focus_to_window(&mut self, window_index: usize, surface: WlSurface) {
        self.focused_normal_window_id = (self.windows[window_index].kind
            == ManagedWindowKind::Normal)
            .then_some(self.windows[window_index].id);
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
    }

    pub(super) fn clear_keyboard_focus(&mut self) {
        self.focused_normal_window_id = None;
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(
            self,
            Option::<WlSurface>::None,
            SERIAL_COUNTER.next_serial(),
        );
    }

    pub(super) fn record_client_activity_for_window_index(
        &self,
        window_index: usize,
        reason: ActivityReason,
    ) {
        let Some(window) = self.windows.get(window_index) else {
            return;
        };
        if window.kind == ManagedWindowKind::Normal {
            self.idle_daemon.record_activity(window.id, reason);
        }
    }

    pub(super) fn record_focused_client_activity(&self, reason: ActivityReason) {
        if let Some(window_id) = self.focused_normal_window_id {
            self.idle_daemon.record_activity(window_id, reason);
        }
    }

    pub(super) fn window_index_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|window| surface_tree_contains(window.surface.wl_surface(), surface))
    }

    pub(super) fn window_mut_by_id(&mut self, window_id: u64) -> Option<&mut ManagedWindow> {
        self.windows
            .iter_mut()
            .find(|window| window.id == window_id)
    }

    pub(super) fn managed_normal_window_id_for_surface(&self, surface: &WlSurface) -> Option<u64> {
        self.windows.iter().find_map(|window| {
            (window.kind == ManagedWindowKind::Normal
                && surface_tree_contains(window.surface.wl_surface(), surface))
            .then_some(window.id)
        })
    }

    /// Recompute and cache the surface-tree bounding box for the window owning
    /// `surface`. Called once per commit so the per-frame rendering and
    /// hit-testing paths can read `content_bbox_size` instead of re-walking the
    /// tree multiple times.
    pub(super) fn refresh_window_content_bbox(&mut self, surface: &WlSurface) {
        let Some(window_index) = self.window_index_for_surface(surface) else {
            return;
        };
        let bbox = bbox_from_surface_tree(
            self.windows[window_index].surface.wl_surface(),
            Point::<i32, Logical>::from((0, 0)),
        );
        self.windows[window_index].content_bbox_size = bbox.size;
    }

    pub(super) fn handle_idle_transitions(&self) {
        for transition in self.idle_daemon.drain_transitions() {
            super::log_idle_transition(transition);
        }
    }

    pub(super) fn accessibility_window_snapshot(&self) -> Vec<ManagedWindowAccessibilityInfo> {
        self.windows
            .iter()
            .filter(|window| window.kind == ManagedWindowKind::Normal)
            .map(|window| ManagedWindowAccessibilityInfo {
                id: window.id,
                app_id: toplevel_app_id(&window.surface),
                title: toplevel_title(&window.surface),
            })
            .collect()
    }

    pub(super) fn hit_test(&self, location: Point<f64, Logical>) -> Option<HitTarget> {
        for (window_index, window) in self.windows.iter().enumerate().rev() {
            if window.kind != ManagedWindowKind::ShellBar {
                continue;
            }

            if let Some(target) = self.hit_test_shell_bar(window_index, location) {
                return Some(target);
            }
        }

        if location.y < f64::from(CONTROL_BAR_HEIGHT) {
            return None;
        }

        let canvas_location = self.screen_to_canvas(location);

        for (window_index, window) in self.windows.iter().enumerate().rev() {
            if window.kind != ManagedWindowKind::Normal {
                continue;
            }

            let content_origin = self.content_canvas_origin(window_index);

            // Popups (menus) sit above this window's content and chrome, so
            // they are hit-tested first. Their location comes from the popup's
            // configured offset relative to the parent surface origin.
            let geometry_loc = toplevel_geometry_loc(window.surface.wl_surface());
            for (popup, popup_offset) in
                PopupManager::popups_for_surface(window.surface.wl_surface())
            {
                let popup_origin =
                    content_origin + geometry_loc + popup_offset - popup.geometry().loc;
                let hit = under_from_surface_tree(
                    popup.wl_surface(),
                    canvas_location,
                    popup_origin,
                    WindowSurfaceType::ALL,
                );
                if let Some((surface, surface_location)) = hit {
                    let relative_surface_location = canvas_location - surface_location.to_f64();
                    let pointer_focus_origin = location - relative_surface_location;
                    return Some(HitTarget::Client {
                        window_index,
                        surface,
                        surface_location: pointer_focus_origin,
                    });
                }
            }

            if self.has_compositor_chrome(window_index) {
                if rect_contains(self.close_button_canvas_rect(window_index), canvas_location) {
                    return Some(HitTarget::CloseButton { window_index });
                }

                if rect_contains(self.title_bar_canvas_rect(window_index), canvas_location) {
                    return Some(HitTarget::TitleBar { window_index });
                }

                // The interactive resize border sits in a frame just outside the
                // window's chrome and content, so it is checked before the
                // client surface tree (which only covers the interior).
                let window_rect = self.window_canvas_rect(window_index);
                if let Some(edges) =
                    resize_edges_at(window_rect, canvas_location, RESIZE_BORDER_THICKNESS)
                {
                    return Some(HitTarget::ResizeBorder {
                        window_index,
                        edges,
                    });
                }
            }

            if let Some((surface, surface_location)) = under_from_surface_tree(
                window.surface.wl_surface(),
                canvas_location,
                content_origin,
                WindowSurfaceType::ALL,
            ) {
                let relative_surface_location = canvas_location - surface_location.to_f64();
                let pointer_focus_origin = location - relative_surface_location;
                return Some(HitTarget::Client {
                    window_index,
                    surface,
                    surface_location: pointer_focus_origin,
                });
            }
        }

        None
    }

    pub(super) fn raise_window(&mut self, window_index: usize) -> usize {
        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return window_index;
        }

        let window = self.windows.remove(window_index);
        let insert_index = self.normal_insert_index();
        self.windows.insert(insert_index, window);
        self.request_redraw();
        insert_index
    }

    pub(super) fn normal_insert_index(&self) -> usize {
        normal_insert_index_for_kinds(self.windows.iter().map(|window| window.kind))
    }

    pub(super) fn configure_toplevel(&self, surface: &ToplevelSurface, kind: ManagedWindowKind) {
        surface.with_pending_state(|state| {
            state
                .capabilities
                .replace(std::iter::empty::<xdg_toplevel::WmCapabilities>());
            if kind == ManagedWindowKind::ShellBar {
                state.size = Some((self.output_size.w, CONTROL_BAR_HEIGHT).into());
                state.bounds = Some((self.output_size.w, CONTROL_BAR_HEIGHT).into());
                state.decoration_mode = Some(DecorationMode::ClientSide);
            } else {
                state.states.set(xdg_toplevel::State::Activated);
            }
        });
        surface.send_configure();
    }

    pub(super) fn configure_shell_bars(&self) {
        for window in &self.windows {
            if window.kind == ManagedWindowKind::ShellBar {
                self.configure_toplevel(&window.surface, window.kind);
            }
        }
    }

    pub(super) fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub(super) fn set_window_decoration(
        &mut self,
        toplevel: &ToplevelSurface,
        decoration: WindowDecoration,
    ) {
        if window_kind_for_toplevel(toplevel) == ManagedWindowKind::ShellBar {
            configure_client_side_decoration(toplevel);
            return;
        }

        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.surface == *toplevel)
        {
            window.decoration = decoration;
        }

        match decoration {
            WindowDecoration::ServerSide => configure_server_side_decoration(toplevel),
            WindowDecoration::ClientSide => configure_client_side_decoration(toplevel),
        }
    }

    pub(super) fn title_bar_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let canvas_rect = self.title_bar_canvas_rect(window_index);
        let origin = self
            .canvas_to_screen(canvas_rect.loc.to_f64())
            .to_i32_round();
        Rectangle::new(
            origin,
            (
                (f64::from(canvas_rect.size.w) * self.viewport_scale).round() as i32,
                (f64::from(canvas_rect.size.h) * self.viewport_scale).round() as i32,
            )
                .into(),
        )
    }

    pub(super) fn title_bar_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        title_bar_canvas_rect_for(window.position, window.content_bbox_size.w)
    }

    fn close_button_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        close_button_canvas_rect_for(self.title_bar_canvas_rect(window_index))
    }

    fn content_canvas_origin(&self, window_index: usize) -> Point<i32, Logical> {
        content_canvas_origin_for(
            self.windows[window_index].position,
            self.has_compositor_chrome(window_index),
        )
    }

    pub(super) fn has_compositor_chrome(&self, window_index: usize) -> bool {
        let window = &self.windows[window_index];
        window.kind == ManagedWindowKind::Normal
            && window.decoration == WindowDecoration::ServerSide
    }

    /// Full canvas-space bounds of a window, including its server-side title bar
    /// (when present) and client content. Used for interactive-resize hit-testing.
    fn window_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        window_canvas_rect_for(
            window.position,
            window.content_bbox_size,
            self.has_compositor_chrome(window_index),
        )
    }

    /// Begin an interactive resize of `window_index` along `edges`, capturing the
    /// window's starting geometry so motion deltas can be resolved against it.
    pub(super) fn start_resize(&mut self, window_index: usize, edges: ResizeEdges) {
        let window = &self.windows[window_index];
        self.resize = Some(ResizeState {
            window_id: window.id,
            edges,
            pointer_start: self.pointer_location,
            initial_position: window.position,
            initial_content_size: window.content_bbox_size,
        });
        self.request_redraw();
    }

    /// Resolve a pointer position during an active resize into a new client
    /// content size and forward it to the window's client.
    pub(super) fn update_resize(&mut self, location: Point<f64, Logical>) {
        let Some(resize) = self.resize.as_ref() else {
            return;
        };
        let edges = resize.edges;
        let initial = resize.initial_content_size;
        let window_id = resize.window_id;
        let delta = location - resize.pointer_start;
        let canvas_delta = Point::<i32, Logical>::from((
            (delta.x / self.viewport_scale).round() as i32,
            (delta.y / self.viewport_scale).round() as i32,
        ));
        let size = resize_target_content_size(edges, initial, canvas_delta);
        self.configure_resize(window_id, size);
    }

    /// Send the in-progress resize size to the resized window's client. The
    /// client applies it on its next commit, where the window is re-anchored.
    pub(super) fn configure_resize(&self, window_id: u64, size: Size<i32, Logical>) {
        let Some(window) = self.windows.iter().find(|window| window.id == window_id) else {
            return;
        };
        window.surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(size);
        });
        window.surface.send_configure();
    }

    /// End the active interactive resize, clearing the `Resizing` state on the
    /// client so it can return to its normal rendering.
    pub(super) fn finish_resize(&mut self) {
        let Some(resize) = self.resize.take() else {
            return;
        };
        if let Some(window) = self
            .windows
            .iter()
            .find(|window| window.id == resize.window_id)
        {
            window.surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
            });
            window.surface.send_configure();
        }
    }

    /// Re-anchor a window being resized from its left/top edge so the opposite
    /// edge stays put as the client's content size changes. Called on commit,
    /// after the new content bounding box has been cached.
    pub(super) fn reanchor_resize(&mut self, surface: &WlSurface) {
        let Some(resize) = self.resize.as_ref() else {
            return;
        };
        if !(resize.edges.left || resize.edges.top) {
            return;
        }
        let Some(window_index) = self.window_index_for_surface(surface) else {
            return;
        };
        if self.windows[window_index].id != resize.window_id {
            return;
        }
        let new_position = resize_anchored_position(
            resize.edges,
            resize.initial_position,
            resize.initial_content_size,
            self.windows[window_index].content_bbox_size,
        );
        self.windows[window_index].position = new_position;
    }
    fn content_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        self.canvas_to_screen(self.content_canvas_origin(window_index).to_f64())
            .to_i32_round()
            .to_physical(1)
    }

    fn shell_bar_screen_origin(&self) -> Point<i32, Physical> {
        Point::<i32, Logical>::from((0, 0)).to_physical(1)
    }

    pub(super) fn surface_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        match self.windows[window_index].kind {
            ManagedWindowKind::Normal => self.content_screen_origin(window_index),
            ManagedWindowKind::ShellBar => self.shell_bar_screen_origin(),
        }
    }

    fn hit_test_shell_bar(
        &self,
        window_index: usize,
        location: Point<f64, Logical>,
    ) -> Option<HitTarget> {
        let window = &self.windows[window_index];
        let (surface, surface_location) = under_from_surface_tree(
            window.surface.wl_surface(),
            location,
            Point::<i32, Logical>::from((0, 0)),
            WindowSurfaceType::ALL,
        )?;
        let relative_surface_location = location - surface_location.to_f64();
        let pointer_focus_origin = location - relative_surface_location;
        Some(HitTarget::Client {
            window_index,
            surface,
            surface_location: pointer_focus_origin,
        })
    }
}

fn surface_tree_contains(root: &wl_surface::WlSurface, target: &wl_surface::WlSurface) -> bool {
    let mut contains = false;
    with_surface_tree_downward(
        root,
        (),
        |surface, _, &()| {
            if surface == target {
                contains = true;
                TraversalAction::Break
            } else {
                TraversalAction::DoChildren(())
            }
        },
        |_, _, &()| {},
        |_, _, &()| true,
    );
    contains
}

/// Index at which a newly raised normal window should be inserted so it sits on
/// top of every other normal window while staying below the shell bars (which
/// are kept at the end of the list).
fn normal_insert_index_for_kinds(kinds: impl Iterator<Item = ManagedWindowKind>) -> usize {
    kinds
        .enumerate()
        .filter_map(|(index, kind)| (kind == ManagedWindowKind::Normal).then_some(index + 1))
        .last()
        .unwrap_or(0)
}

/// Canvas-space rectangle of a window's server-side title bar, given the
/// window's canvas position and the width of its content surface tree.
fn title_bar_canvas_rect_for(position: CanvasPoint, content_width: i32) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (position.x, position.y).into(),
        (content_width.max(MIN_WINDOW_WIDTH), TITLE_BAR_HEIGHT).into(),
    )
}

/// Canvas-space rectangle of the close button, positioned at the right edge of
/// the given title bar and vertically centered within it.
fn close_button_canvas_rect_for(title_bar: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (
            title_bar.loc.x + title_bar.size.w - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE,
            title_bar.loc.y + (title_bar.size.h - CLOSE_BUTTON_SIZE) / 2,
        )
            .into(),
        (CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE).into(),
    )
}

/// Canvas-space origin of a window's client content, offset below the title bar
/// only when the compositor is drawing server-side chrome for it.
fn content_canvas_origin_for(position: CanvasPoint, has_chrome: bool) -> Point<i32, Logical> {
    let title_bar_height = if has_chrome { TITLE_BAR_HEIGHT } else { 0 };
    Point::<i32, Logical>::from((position.x, position.y + title_bar_height))
}

/// Full canvas-space bounds of a window given its position, client content size
/// and whether the compositor draws server-side chrome (a title bar above the
/// content). The width tracks the title bar, which is clamped to a minimum.
fn window_canvas_rect_for(
    position: CanvasPoint,
    content_size: Size<i32, Logical>,
    has_chrome: bool,
) -> Rectangle<i32, Logical> {
    let title_bar_height = if has_chrome { TITLE_BAR_HEIGHT } else { 0 };
    let width = if has_chrome {
        content_size.w.max(MIN_WINDOW_WIDTH)
    } else {
        content_size.w
    };
    Rectangle::new(
        (position.x, position.y).into(),
        (width, content_size.h + title_bar_height).into(),
    )
}

/// Determine which resize edges, if any, the pointer is over. The interactive
/// resize region is a frame of `border` pixels just outside `window_rect`;
/// points inside the window (its chrome/content) are not resize targets.
fn resize_edges_at(
    window_rect: Rectangle<i32, Logical>,
    point: Point<f64, Logical>,
    border: i32,
) -> Option<ResizeEdges> {
    let outer = Rectangle::new(
        (window_rect.loc.x - border, window_rect.loc.y - border).into(),
        (
            window_rect.size.w + border * 2,
            window_rect.size.h + border * 2,
        )
            .into(),
    );
    if !rect_contains(outer, point) || rect_contains(window_rect, point) {
        return None;
    }
    let edges = ResizeEdges {
        left: point.x < f64::from(window_rect.loc.x),
        right: point.x >= f64::from(window_rect.loc.x + window_rect.size.w),
        top: point.y < f64::from(window_rect.loc.y),
        bottom: point.y >= f64::from(window_rect.loc.y + window_rect.size.h),
    };
    (!edges.is_empty()).then_some(edges)
}

/// Desired client content size for a resize drag, derived from the size at the
/// start of the resize and the canvas-space pointer delta. Edges that are not
/// being dragged leave their dimension unchanged; the result is clamped to the
/// minimum window dimensions.
fn resize_target_content_size(
    edges: ResizeEdges,
    initial: Size<i32, Logical>,
    delta: Point<i32, Logical>,
) -> Size<i32, Logical> {
    let mut width = initial.w;
    let mut height = initial.h;
    if edges.left {
        width = initial.w - delta.x;
    }
    if edges.right {
        width = initial.w + delta.x;
    }
    if edges.top {
        height = initial.h - delta.y;
    }
    if edges.bottom {
        height = initial.h + delta.y;
    }
    (width.max(MIN_WINDOW_WIDTH), height.max(MIN_WINDOW_HEIGHT)).into()
}

/// Window position that keeps the edge opposite a left/top resize anchored in
/// place as the client's content size changes from `initial` to `actual`.
fn resize_anchored_position(
    edges: ResizeEdges,
    anchor: CanvasPoint,
    initial: Size<i32, Logical>,
    actual: Size<i32, Logical>,
) -> CanvasPoint {
    let mut position = anchor;
    if edges.left {
        position.x = anchor.x + initial.w - actual.w;
    }
    if edges.top {
        position.y = anchor.y + initial.h - actual.h;
    }
    position
}

/// The cursor that communicates which resize a window edge or corner performs.
/// Edges map to the bidirectional CSS-style resize cursors; corners map to the
/// matching diagonal cursor.
pub(super) fn resize_cursor_icon(edges: ResizeEdges) -> CursorIcon {
    match (edges.top || edges.bottom, edges.left || edges.right) {
        (true, true) => {
            if (edges.top && edges.left) || (edges.bottom && edges.right) {
                CursorIcon::NwseResize
            } else {
                CursorIcon::NeswResize
            }
        }
        (true, false) => CursorIcon::NsResize,
        (false, true) => CursorIcon::EwResize,
        (false, false) => CursorIcon::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> CanvasPoint {
        CanvasPoint { x, y }
    }

    #[test]
    fn title_bar_uses_content_width_when_wider_than_minimum() {
        let rect = title_bar_canvas_rect_for(point(40, 50), MIN_WINDOW_WIDTH + 120);
        assert_eq!(rect.loc.x, 40);
        assert_eq!(rect.loc.y, 50);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH + 120);
        assert_eq!(rect.size.h, TITLE_BAR_HEIGHT);
    }

    #[test]
    fn title_bar_clamps_to_minimum_window_width() {
        let rect = title_bar_canvas_rect_for(point(0, 0), 10);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn close_button_sits_inside_the_title_bar_right_edge() {
        let title_bar = title_bar_canvas_rect_for(point(100, 200), 400);
        let close = close_button_canvas_rect_for(title_bar);

        assert_eq!(close.size.w, CLOSE_BUTTON_SIZE);
        assert_eq!(close.size.h, CLOSE_BUTTON_SIZE);
        // Right edge respects the margin.
        assert_eq!(
            close.loc.x + close.size.w,
            title_bar.loc.x + title_bar.size.w - CLOSE_BUTTON_MARGIN
        );
        // Fully contained within the title bar vertically.
        assert!(close.loc.y >= title_bar.loc.y);
        assert!(close.loc.y + close.size.h <= title_bar.loc.y + title_bar.size.h);
    }

    #[test]
    fn content_origin_drops_below_title_bar_only_with_chrome() {
        assert_eq!(
            content_canvas_origin_for(point(10, 20), true),
            Point::<i32, Logical>::from((10, 20 + TITLE_BAR_HEIGHT))
        );
        assert_eq!(
            content_canvas_origin_for(point(10, 20), false),
            Point::<i32, Logical>::from((10, 20))
        );
    }

    #[test]
    fn insert_index_is_after_the_last_normal_window() {
        use ManagedWindowKind::{Normal, ShellBar};
        assert_eq!(normal_insert_index_for_kinds([].into_iter()), 0);
        assert_eq!(
            normal_insert_index_for_kinds([ShellBar].into_iter()),
            0,
            "with only shell bars, normals go to the front"
        );
        assert_eq!(
            normal_insert_index_for_kinds([Normal, Normal, ShellBar].into_iter()),
            2,
            "insert above the topmost normal but below the shell bar"
        );
        assert_eq!(
            normal_insert_index_for_kinds([Normal, ShellBar, Normal].into_iter()),
            3
        );
    }

    fn size(w: i32, h: i32) -> Size<i32, Logical> {
        (w, h).into()
    }

    fn fpoint(x: f64, y: f64) -> Point<f64, Logical> {
        (x, y).into()
    }

    fn edges(left: bool, right: bool, top: bool, bottom: bool) -> ResizeEdges {
        ResizeEdges {
            left,
            right,
            top,
            bottom,
        }
    }

    #[test]
    fn resize_edges_from_protocol_corners_set_two_sides() {
        use xdg_toplevel::ResizeEdge;
        assert_eq!(
            ResizeEdges::from(ResizeEdge::TopLeft),
            edges(true, false, true, false)
        );
        assert_eq!(
            ResizeEdges::from(ResizeEdge::BottomRight),
            edges(false, true, false, true)
        );
        assert_eq!(
            ResizeEdges::from(ResizeEdge::Right),
            edges(false, true, false, false)
        );
        assert!(ResizeEdges::from(ResizeEdge::None).is_empty());
    }

    #[test]
    fn window_rect_adds_title_bar_height_only_with_chrome() {
        let with_chrome = window_canvas_rect_for(point(10, 20), size(400, 300), true);
        assert_eq!(with_chrome.loc, Point::<i32, Logical>::from((10, 20)));
        assert_eq!(with_chrome.size.w, 400);
        assert_eq!(with_chrome.size.h, 300 + TITLE_BAR_HEIGHT);

        let without_chrome = window_canvas_rect_for(point(10, 20), size(400, 300), false);
        assert_eq!(without_chrome.size.h, 300);
    }

    #[test]
    fn window_rect_width_clamps_to_minimum_with_chrome() {
        let rect = window_canvas_rect_for(point(0, 0), size(10, 200), true);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn resize_edges_at_detects_corner_outside_window() {
        let rect = Rectangle::new((100, 100).into(), (200, 150).into());
        // Just outside the top-left corner.
        assert_eq!(
            resize_edges_at(rect, fpoint(96.0, 96.0), 8),
            Some(edges(true, false, true, false))
        );
        // Along the right edge only.
        assert_eq!(
            resize_edges_at(rect, fpoint(303.0, 175.0), 8),
            Some(edges(false, true, false, false))
        );
    }

    #[test]
    fn resize_edges_at_ignores_interior_and_far_points() {
        let rect = Rectangle::new((100, 100).into(), (200, 150).into());
        // Inside the window content.
        assert_eq!(resize_edges_at(rect, fpoint(150.0, 150.0), 8), None);
        // Beyond the border frame.
        assert_eq!(resize_edges_at(rect, fpoint(50.0, 50.0), 8), None);
    }

    #[test]
    fn resize_target_grows_with_bottom_right_drag() {
        let result = resize_target_content_size(
            edges(false, true, false, true),
            size(400, 300),
            (60, 40).into(),
        );
        assert_eq!(result, size(460, 340));
    }

    #[test]
    fn resize_target_shrinks_with_left_drag_and_clamps() {
        // Dragging the left edge right shrinks the width.
        let result = resize_target_content_size(
            edges(true, false, false, false),
            size(400, 300),
            (30, 0).into(),
        );
        assert_eq!(result, size(370, 300));
        // Clamped to the minimum width regardless of how far the drag goes.
        let clamped = resize_target_content_size(
            edges(true, false, false, false),
            size(400, 300),
            (10_000, 0).into(),
        );
        assert_eq!(clamped.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn anchored_position_keeps_right_and_bottom_fixed_on_left_top_resize() {
        // Initial right edge = 100 + 400 = 500, bottom = 100 + 300 = 400.
        let position = resize_anchored_position(
            edges(true, false, true, false),
            point(100, 100),
            size(400, 300),
            size(450, 320),
        );
        // New width 450 keeps right at 500 -> x = 50. New height 320 keeps bottom at 400 -> y = 80.
        assert_eq!(position, point(50, 80));
    }

    #[test]
    fn anchored_position_unchanged_for_right_bottom_resize() {
        let position = resize_anchored_position(
            edges(false, true, false, true),
            point(100, 100),
            size(400, 300),
            size(450, 320),
        );
        assert_eq!(position, point(100, 100));
    }

    #[test]
    fn resize_cursor_matches_edges_and_corners() {
        assert_eq!(
            resize_cursor_icon(edges(false, false, true, false)),
            CursorIcon::NsResize
        );
        assert_eq!(
            resize_cursor_icon(edges(false, false, false, true)),
            CursorIcon::NsResize
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, false, false)),
            CursorIcon::EwResize
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, true, false)),
            CursorIcon::NwseResize,
            "top-left corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(false, true, false, true)),
            CursorIcon::NwseResize,
            "bottom-right corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(false, true, true, false)),
            CursorIcon::NeswResize,
            "top-right corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, false, true)),
            CursorIcon::NeswResize,
            "bottom-left corner"
        );
        assert_eq!(
            resize_cursor_icon(ResizeEdges::default()),
            CursorIcon::Default
        );
    }
}
