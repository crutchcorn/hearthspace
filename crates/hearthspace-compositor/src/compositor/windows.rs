use smithay::{
    desktop::utils::bbox_from_surface_tree,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Size},
    wayland::{
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
        selection::{data_device::set_data_device_focus, primary_selection::set_primary_focus},
        shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData},
    },
};
use tracing::{debug, trace};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    shell::server::xdg_toplevel,
};
use wayland_server::protocol::wl_surface::{self, WlSurface};

use crate::{accessibility::ManagedWindowAccessibilityInfo, config::*, geometry::CanvasPoint};
use hearthspace_common::{
    control_osd_output_name_from_app_id, shell_bar_output_name_from_app_id,
    shell_panel_output_name_from_app_id,
};

mod bounds;
mod control_osd;
mod geometry;
mod hit_testing;
mod ordering;
mod shell_corners;
mod shell_panels;
mod shell_position;
mod shell_size;
mod titlebar_controls;

pub(super) use geometry::{ResizeEdges, resize_cursor_icon};
use ordering::normal_insert_index_for_kinds;
pub(super) use titlebar_controls::{TitlebarButton, titlebar_button_rect};

use geometry::{
    content_canvas_origin_for, resize_anchored_position, resize_target_content_size,
    title_bar_canvas_rect_for, window_canvas_rect_for,
};

use super::{
    App, ManagedSurface, ManagedWindow, ManagedWindowKind, ResizeState, WindowDecoration,
    focus::KeyboardFocusTarget,
    idle::ActivityReason,
    rendering::{toplevel_geometry_loc, toplevel_geometry_size},
};

fn configure_server_side_decoration(toplevel: &ToplevelSurface) {
    if window_kind_for_toplevel(toplevel).is_shell_chrome() {
        return;
    }

    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_configure();
    }
}

fn configure_client_side_decoration(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ClientSide);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_configure();
    }
}

pub(super) fn window_kind_for_toplevel(surface: &ToplevelSurface) -> ManagedWindowKind {
    let app_id = toplevel_app_id(surface);
    if app_id
        .as_deref()
        .and_then(shell_bar_output_name_from_app_id)
        .is_some()
    {
        return ManagedWindowKind::ShellBar;
    }
    if app_id
        .as_deref()
        .and_then(shell_panel_output_name_from_app_id)
        .is_some()
    {
        return ManagedWindowKind::Launcher;
    }
    if app_id
        .as_deref()
        .and_then(control_osd_output_name_from_app_id)
        .is_some()
    {
        return ManagedWindowKind::ControlOsd;
    }
    if app_id.as_deref() == Some(SHELL_OUTPUT_WATCHER_APP_ID) {
        return ManagedWindowKind::ShellWatcher;
    }
    ManagedWindowKind::Normal
}

pub(super) fn shell_output_name_for_toplevel(surface: &ToplevelSurface) -> Option<String> {
    let app_id = toplevel_app_id(surface)?;
    shell_bar_output_name_from_app_id(&app_id)
        .or_else(|| shell_panel_output_name_from_app_id(&app_id))
        .or_else(|| control_osd_output_name_from_app_id(&app_id))
        .flatten()
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

pub(super) fn window_title(surface: &ManagedSurface) -> Option<String> {
    match surface {
        ManagedSurface::Xdg(surface) => toplevel_title(surface),
        #[cfg(feature = "xwayland")]
        ManagedSurface::X11(surface) => {
            let title = surface.title();
            (!title.trim().is_empty()).then_some(title)
        }
    }
}

pub(super) fn position_for_new_window(
    kind: ManagedWindowKind,
    fallback: CanvasPoint,
) -> CanvasPoint {
    match kind {
        ManagedWindowKind::Normal => fallback,
        ManagedWindowKind::ShellBar => CanvasPoint { x: 0, y: 0 },
        ManagedWindowKind::ShellWatcher | ManagedWindowKind::ControlOsd => {
            CanvasPoint { x: 0, y: 0 }
        }
        ManagedWindowKind::Launcher => CanvasPoint {
            x: 0,
            y: CONTROL_BAR_HEIGHT,
        },
    }
}

pub(super) fn decoration_for_new_window(kind: ManagedWindowKind) -> WindowDecoration {
    match kind {
        ManagedWindowKind::Normal
        | ManagedWindowKind::ShellBar
        | ManagedWindowKind::Launcher
        | ManagedWindowKind::ShellWatcher
        | ManagedWindowKind::ControlOsd => WindowDecoration::ClientSide,
    }
}

impl App {
    pub(super) fn set_keyboard_focus_to_window(&mut self, window_index: usize, surface: WlSurface) {
        if matches!(
            self.windows[window_index].kind,
            ManagedWindowKind::ControlOsd | ManagedWindowKind::ShellWatcher
        ) || self.session_locked()
            || self.overview_active()
            || (self.window_switcher_active()
                && self.window_switcher_selected_id() != Some(self.windows[window_index].id))
        {
            return;
        }
        debug!(window_id = self.windows[window_index].id, kind = ?self.windows[window_index].kind, "setting keyboard focus to window");
        let focus = KeyboardFocusTarget::for_window(&self.windows[window_index].surface, surface);
        let keyboard = self.keyboard.clone();
        let previous = keyboard.current_focus();
        keyboard.set_focus(self, Some(focus), SERIAL_COUNTER.next_serial());
        let current = keyboard.current_focus();
        // X11 may request activation before its first buffer maps. Smithay
        // skips focus_changed when the target stays the same, so refresh the
        // mapped/activated bookkeeping even on that transition.
        if current == previous {
            self.sync_window_activation(current.as_ref().map(KeyboardFocusTarget::surface));
        }
    }

    pub(super) fn clear_keyboard_focus(&mut self) {
        debug!("clearing keyboard focus");
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(
            self,
            Option::<KeyboardFocusTarget>::None,
            SERIAL_COUNTER.next_serial(),
        );
        // This Smithay revision calls focus_changed only for a new surface,
        // not when clearing focus. A grab may defer the keyboard change, so
        // revoke clipboard focus only once the keyboard is actually unfocused.
        if keyboard.current_focus().is_none() {
            self.sync_window_activation(None);
            set_data_device_focus(&self.display_handle, &self._seat, None);
            set_primary_focus(&self.display_handle, &self._seat, None);
        }
    }

    pub(super) fn record_client_activity_for_window_index(
        &self,
        window_index: usize,
        reason: ActivityReason,
    ) {
        if self.session_locked() {
            return;
        }
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
        self.windows.iter().position(|window| {
            window
                .surface
                .wl_surface()
                .as_ref()
                .is_some_and(|root| surface_tree_contains(root, surface))
        })
    }

    pub(super) fn window_mut_by_id(&mut self, window_id: u64) -> Option<&mut ManagedWindow> {
        self.windows
            .iter_mut()
            .find(|window| window.id == window_id)
    }

    pub(super) fn managed_normal_window_id_for_surface(&self, surface: &WlSurface) -> Option<u64> {
        self.windows.iter().find_map(|window| {
            (window.kind == ManagedWindowKind::Normal
                && window
                    .surface
                    .wl_surface()
                    .as_ref()
                    .is_some_and(|root| surface_tree_contains(root, surface)))
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
        if let Some(root) = self.windows[window_index].surface.wl_surface() {
            let bbox = bbox_from_surface_tree(&root, Point::<i32, Logical>::from((0, 0)));
            self.windows[window_index].content_bbox_size = bbox.size;
        }
    }

    pub(super) fn refresh_normal_window_outputs(&self) {
        for (window_index, window) in self.windows.iter().enumerate() {
            if window.kind != ManagedWindowKind::Normal {
                continue;
            }
            let Some(rect) = self.normal_window_screen_rect(window_index) else {
                continue;
            };
            if let Some(surface) = window.surface.wl_surface() {
                self.enter_outputs_intersecting(&surface, rect);
            }
        }
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
                app_id: window_app_id(&window.surface),
                title: window_title(&window.surface),
            })
            .collect()
    }

    pub(super) fn raise_window(&mut self, window_index: usize) -> usize {
        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return window_index;
        }

        let window_id = self.windows[window_index].id;
        let children: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.transient_descendant_of(*index, window_id))
            .map(|(_, window)| window.id)
            .collect();
        let mut family = vec![self.windows.remove(window_index)];
        for child in children {
            if let Some(index) = self.windows.iter().position(|window| window.id == child) {
                family.push(self.windows.remove(index));
            }
        }
        let insert_index = self.normal_insert_index();
        for (offset, window) in family.into_iter().enumerate() {
            self.windows.insert(insert_index + offset, window);
        }
        debug!(
            window_id,
            from_index = window_index,
            to_index = insert_index,
            "raised window"
        );
        self.request_redraw();
        insert_index
    }

    pub(super) fn normal_insert_index(&self) -> usize {
        normal_insert_index_for_kinds(self.windows.iter().map(|window| window.kind))
    }

    pub(super) fn configure_toplevel(
        &self,
        surface: &ToplevelSurface,
        kind: ManagedWindowKind,
        shell_output_name: Option<&str>,
    ) {
        trace!(?kind, "configuring xdg toplevel");
        let normal_bounds = (kind == ManagedWindowKind::Normal)
            .then(|| self.normal_toplevel_bounds(surface))
            .flatten();
        let bar_size = (kind == ManagedWindowKind::ShellBar).then(|| {
            let output = self
                .output_client_logical_size_for_name(shell_output_name)
                .unwrap_or_else(|| (self.minimum_output_client_logical_width(), 800).into());
            let corner = toplevel_app_id(surface)
                .as_deref()
                .and_then(hearthspace_common::shell_corner_from_app_id);
            corner
                .map(|corner| {
                    shell_corners::corner_size(
                        corner,
                        shell_size::fixed_size(surface.wl_surface()),
                        output,
                    )
                })
                .unwrap_or_else(|| (output.w, CONTROL_BAR_HEIGHT).into())
        });
        surface.with_pending_state(|state| {
            state
                .capabilities
                .replace(if kind == ManagedWindowKind::Normal {
                    vec![
                        xdg_toplevel::WmCapabilities::Maximize,
                        xdg_toplevel::WmCapabilities::Fullscreen,
                    ]
                } else {
                    Vec::new()
                });
            match kind {
                ManagedWindowKind::ShellBar => {
                    state.size = bar_size;
                    state.bounds = bar_size;
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::Launcher | ManagedWindowKind::ControlOsd => {
                    // Content measurements arrive as fixed min/max hints. The
                    // palette sizing path then applies the output bounds and
                    // responsive search width.
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::ShellWatcher => {
                    state.size = Some((1, 1).into());
                    state.bounds = Some((1, 1).into());
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::Normal => state.bounds = normal_bounds,
            }
        });
        // The initial empty wl_surface.commit establishes the client's parent,
        // decoration and size hints before its first configure is sent.
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    #[cfg_attr(not(feature = "winit"), allow(dead_code))]
    pub(super) fn configure_shell_bars(&mut self) {
        for window in &self.windows {
            if window.kind.is_shell_chrome()
                && let Some(surface) = window.surface.wl_surface()
            {
                self.enter_shell_output(&surface, window.shell_output_name.as_deref());
            }
            if window.kind == ManagedWindowKind::ShellBar
                && let Some(surface) = window.surface.xdg_toplevel()
            {
                self.configure_toplevel(surface, window.kind, window.shell_output_name.as_deref());
            }
            if matches!(
                window.kind,
                ManagedWindowKind::ShellBar
                    | ManagedWindowKind::Launcher
                    | ManagedWindowKind::ControlOsd
            ) && let Some(surface) = window.surface.wl_surface()
            {
                self.configure_shell_palette_size(&surface);
            }
        }
        self.reconcile_window_presentations();
        self.reconfigure_lock_outputs();
    }

    pub(super) fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub(super) fn set_window_decoration(
        &mut self,
        toplevel: &ToplevelSurface,
        decoration: WindowDecoration,
    ) {
        if window_kind_for_toplevel(toplevel).is_shell_chrome() {
            configure_client_side_decoration(toplevel);
            return;
        }

        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.surface.same_xdg(toplevel))
        {
            window.decoration = decoration;
        }

        match decoration {
            WindowDecoration::ServerSide => configure_server_side_decoration(toplevel),
            WindowDecoration::ClientSide => configure_client_side_decoration(toplevel),
        }
    }

    pub(super) fn title_bar_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        if self.window_uses_output_coordinates(window_index) {
            let rect = self.window_screen_bounds(window_index);
            return Rectangle::new(rect.loc, (rect.size.w, TITLE_BAR_HEIGHT).into());
        }
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

    pub(super) fn titlebar_button_screen_rect(
        &self,
        window_index: usize,
        button: TitlebarButton,
    ) -> Rectangle<f64, Logical> {
        let titlebar = self.title_bar_rect(window_index);
        let scale = self.window_render_scale(window_index);
        let width = if self.window_uses_output_coordinates(window_index) {
            titlebar.size.w
        } else {
            self.title_bar_canvas_rect(window_index).size.w
        };
        let button = titlebar_button_rect(width, button);
        Rectangle::new(
            (
                f64::from(titlebar.loc.x) + f64::from(button.loc.x) * scale,
                f64::from(titlebar.loc.y) + f64::from(button.loc.y) * scale,
            )
                .into(),
            (
                f64::from(button.size.w) * scale,
                f64::from(button.size.h) * scale,
            )
                .into(),
        )
    }

    pub(super) fn content_canvas_origin(&self, window_index: usize) -> Point<i32, Logical> {
        content_canvas_origin_for(
            self.windows[window_index].position,
            self.has_compositor_chrome(window_index),
        )
    }

    pub(super) fn has_compositor_chrome(&self, window_index: usize) -> bool {
        let window = &self.windows[window_index];
        window.kind == ManagedWindowKind::Normal
            && window.decoration == WindowDecoration::ServerSide
            && !window.presentation.fullscreen
    }

    /// The window's interactive size: the xdg window geometry size when the
    /// client has set one (client-side-decorated apps inset their visible window
    /// from the surface bounds with shadow margins), falling back to the
    /// surface-tree bounding box otherwise.
    pub(super) fn window_geometry_size(&self, window_index: usize) -> Size<i32, Logical> {
        let window = &self.windows[window_index];
        self.window_surface_geometry_size(window_index)
            .filter(|size| size.w > 0 && size.h > 0)
            .unwrap_or(window.content_bbox_size)
    }

    /// Full canvas-space bounds of a window's interactive area, including its
    /// server-side title bar when present. For client-side-decorated windows the
    /// rect tracks the *visible* window geometry (excluding shadow margins) so
    /// resize handles land on the visible edge rather than out in the shadow.
    pub(super) fn window_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        if self.has_compositor_chrome(window_index) {
            return window_canvas_rect_for(window.position, window.content_bbox_size, true);
        }
        let surface_origin = self.content_canvas_origin(window_index);
        let geometry_loc = self.window_surface_geometry_loc(window_index);
        Rectangle::new(
            surface_origin + geometry_loc,
            self.window_geometry_size(window_index),
        )
    }

    pub(super) fn normal_window_screen_rect(
        &self,
        window_index: usize,
    ) -> Option<Rectangle<i32, Logical>> {
        if self.windows[window_index].kind != ManagedWindowKind::Normal
            || !self.windows[window_index].mapped
        {
            return None;
        }

        let rect = self.window_screen_bounds(window_index);
        if rect.size.w <= 0 || rect.size.h <= 0 {
            return None;
        }
        Some(rect)
    }

    /// Begin an interactive resize of `window_index` along `edges`, capturing the
    /// window's starting geometry so motion deltas can be resolved against it.
    pub(super) fn start_resize(&mut self, window_index: usize, edges: ResizeEdges) {
        if self.window_uses_output_coordinates(window_index) {
            return;
        }
        self.cancel_group_window_motion(self.windows[window_index].id);
        self.prepare_window_pointer_operation();
        let pointer_start = self.window_pointer_start(window_index);
        let initial_content_size = self.window_geometry_size(window_index);
        let window = &self.windows[window_index];
        debug!(
            window_id = window.id,
            ?edges,
            ?initial_content_size,
            "starting interactive resize"
        );
        self.resize = Some(ResizeState {
            window_id: window.id,
            edges,
            pointer_start,
            initial_position: window.position,
            initial_content_size,
        });
        if pointer_start != self.raw_pointer_location {
            self.update_resize(self.raw_pointer_location);
        }
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
        trace!(window_id, ?edges, ?size, "configuring interactive resize");
        self.configure_resize(window_id, size);
    }

    /// Send the in-progress resize size to the resized window's client. The
    /// client applies it on its next commit, where the window is re-anchored.
    pub(super) fn configure_resize(&self, window_id: u64, size: Size<i32, Logical>) {
        let Some(window) = self.windows.iter().find(|window| window.id == window_id) else {
            return;
        };
        match &window.surface {
            ManagedSurface::Xdg(surface) => {
                surface.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Resizing);
                    state.bounds = None;
                    state.size = Some(size);
                });
                // Motion can round or clamp to an unchanged logical size.
                // Smithay compares with the latest pending/acknowledged state,
                // so identical samples need not enqueue another client frame.
                surface.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => {
                let rect =
                    Rectangle::new(Point::from((window.position.x, window.position.y)), size);
                if let Err(error) = surface.configure(Some(rect)) {
                    tracing::warn!(%error, "failed to configure X11 resize");
                }
            }
        }
    }

    /// End the active interactive resize, clearing the `Resizing` state on the
    /// client so it can return to its normal rendering.
    pub(super) fn finish_resize(&mut self) {
        let Some(resize) = self.resize.take() else {
            return;
        };
        debug!(window_id = resize.window_id, "finishing interactive resize");
        if let Some(window) = self
            .windows
            .iter()
            .find(|window| window.id == resize.window_id)
            && let Some(surface) = window.surface.xdg_toplevel()
        {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
            });
            surface.send_configure();
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
            self.window_geometry_size(window_index),
        );
        self.windows[window_index].position = new_position;
    }
    pub(super) fn window_surface_geometry_loc(&self, window_index: usize) -> Point<i32, Logical> {
        match &self.windows[window_index].surface {
            ManagedSurface::Xdg(surface) => toplevel_geometry_loc(surface.wl_surface()),
            #[cfg(feature = "xwayland")]
            // X11 geometry is in root-window coordinates. It is not an inset
            // within the wl_surface as xdg window geometry is.
            ManagedSurface::X11(_) => Point::from((0, 0)),
        }
    }

    fn window_surface_geometry_size(&self, window_index: usize) -> Option<Size<i32, Logical>> {
        match &self.windows[window_index].surface {
            ManagedSurface::Xdg(surface) => toplevel_geometry_size(surface.wl_surface()),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => Some(surface.geometry().size),
        }
    }
}

pub(super) fn window_app_id(surface: &ManagedSurface) -> Option<String> {
    match surface {
        ManagedSurface::Xdg(surface) => toplevel_app_id(surface),
        #[cfg(feature = "xwayland")]
        ManagedSurface::X11(surface) => {
            let class = surface.class();
            (!class.trim().is_empty()).then_some(class)
        }
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
