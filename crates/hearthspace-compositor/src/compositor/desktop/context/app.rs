//! App catalog menus use the mapped source's placement and existing menu lifetime.
use crate::compositor::{
    App, CalloopData, ManagedWindowKind, shell_integration::ControlReply, windows::window_app_id,
};
use hearthspace_common::{desktop_context, frame_layout, launcher_output_name_from_app_id};
use smithay::utils::{Logical, Point, Rectangle};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    app_id: String,
    output_name: Option<String>,
    source_app_id: String,
    popup_epoch: u64,
    #[serde(default)]
    intent: Intent,
    anchor: Anchor,
}

#[derive(Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Intent {
    #[default]
    Ordinary,
    Accessibility,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl CalloopData {
    pub(in crate::compositor) fn run_desktop_app_context_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        let argument = text.strip_prefix("desktop-app-context ")?;
        if !self.running || !self.capture_active() || self.state.session_locked() {
            return Some(ControlReply::Err("Desktop is inactive".into()));
        }
        let result = serde_json::from_str::<Request>(argument)
            .map_err(|_| "Invalid application menu request".to_string())
            .and_then(|request| self.state.open_app_context(request));
        Some(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok))
    }
}

impl App {
    /// The shell worker serializes full catalog refreshes with app-menu requests.
    /// Offset zero replaces the previous IDs; later batches must be contiguous.
    /// This catalog grants no desktop placement or file-operation authority.
    pub(in crate::compositor::desktop) fn register_desktop_app_ids(
        &mut self,
        offset: usize,
        ids: Vec<String>,
    ) -> Result<(), String> {
        let catalog = &mut self.desktop.context.app_ids;
        if offset != 0 && offset != catalog.len() {
            return Err("application registration offset is stale".into());
        }
        let mut batch = std::collections::BTreeSet::new();
        for id in ids {
            if id.is_empty()
                || hearthspace_common::desktop_item_id_from_app_id(
                    &hearthspace_common::desktop_item_app_id(&format!("app:{id}")),
                )
                .is_none()
            {
                return Err("invalid application registration".into());
            }
            if (offset != 0 && catalog.contains(&id)) || !batch.insert(id) {
                return Err("duplicate application registration".into());
            }
        }
        if offset == 0 {
            *catalog = batch;
        } else {
            catalog.extend(batch);
        }
        Ok(())
    }

    pub(super) fn app_context_item_available(&self, id: &str) -> bool {
        id.strip_prefix("app:")
            .is_some_and(|id| self.desktop.context.app_ids.contains(id))
    }

    fn open_app_context(&mut self, request: Request) -> Result<(), String> {
        self.check_navigation_available()?;
        if request.popup_epoch != self.shell_popup_epoch() {
            return Err("Application menu request was cancelled".into());
        }
        if self.touch.is_grabbed() || self.desktop.gesture.is_some() {
            return Err("Application menu unavailable during an input sequence".into());
        }
        let item = format!("app:{}", request.app_id);
        if !self.app_context_item_available(&item) {
            return Err("Application is no longer available".into());
        }
        let source = self
            .windows
            .iter()
            .position(|window| {
                window.mapped
                    && window_app_id(&window.surface).as_deref()
                        == Some(request.source_app_id.as_str())
            })
            .ok_or("Application menu source is no longer available")?;
        if !self.app_context_source_kind(source)
            || (request.intent == Intent::Ordinary
                && self.keyboard.current_focus().is_none_or(|focus| {
                    self.window_index_for_surface(focus.surface()) != Some(source)
                }))
        {
            return Err("Application menu source no longer owns focus".into());
        }
        let outputs = self.capture_outputs();
        let output_name = request
            .output_name
            .as_deref()
            .or_else(|| outputs.first().map(|output| output.name.as_str()))
            .ok_or("Application menu output is unavailable")?;
        let source_output = self.windows[source]
            .shell_output_name
            .as_deref()
            .or_else(|| outputs.first().map(|output| output.name.as_str()));
        let output = self
            .output_logical_rect_for_name(Some(output_name))
            .ok_or("Application menu output is unavailable")?;
        if source_output != Some(output_name)
            || !self.window_visible_on_output(source, output)
            || !self.shell_visible_on_output(output)
        {
            return Err("Application menu source is not visible on this output".into());
        }
        let anchor = self.app_context_anchor(source, request.anchor, output)?;
        let output_name = output_name.to_owned();
        let accessibility_focus = if request.intent == Intent::Accessibility {
            Some(
                self.windows[source]
                    .surface
                    .wl_surface()
                    .ok_or("Application menu source is no longer available")?,
            )
        } else {
            None
        };
        if let Some(previous) = &self.desktop.context.current {
            self.close_desktop_context(previous.generation);
        }
        // ShowContextMenu is an explicit action on this visible shell control.
        // Acquire its Wayland focus only after every admission check succeeds,
        // in the same compositor dispatch that creates the menu. Ordinary input
        // must already own this focus; this is not a general activation command.
        if let Some(surface) = accessibility_focus {
            self.set_keyboard_focus_to_window(source, surface);
        }
        self.desktop.context.sequence = self.desktop.context.sequence.wrapping_add(1).max(1);
        self.desktop.context.anchor = anchor;
        self.desktop.context.return_focus = None;
        self.desktop.context.source_window = Some(self.windows[source].id);
        self.desktop.context.root_window = None;
        self.desktop.context.current = Some(desktop_context::DesktopContext {
            generation: self.desktop.context.sequence,
            ids: vec![item],
            output: Some(output_name),
            max_width: output.size.w.saturating_sub(16).max(1),
            max_height: output.size.h.saturating_sub(64).max(1),
        });
        self.desktop.last_click = None;
        self.desktop_changed();
        Ok(())
    }

    fn app_context_source_kind(&self, index: usize) -> bool {
        let Some(id) = window_app_id(&self.windows[index].surface) else {
            return false;
        };
        launcher_output_name_from_app_id(&id).is_some()
            || frame_layout::frame_widget_from_app_id(&id).is_some_and(|(_, id)| id == "apps")
    }

    fn app_context_anchor(
        &self,
        source: usize,
        anchor: Anchor,
        output: Rectangle<i32, Logical>,
    ) -> Result<Point<i32, Logical>, String> {
        if [anchor.x, anchor.y, anchor.width, anchor.height]
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 1_000_000.)
            || anchor.width <= 0.
            || anchor.height <= 0.
        {
            return Err("Invalid application menu anchor".into());
        }
        let rect = Rectangle::new(
            Point::<f64, Logical>::from((anchor.x, anchor.y)),
            (anchor.width, anchor.height).into(),
        );
        let geometry = Rectangle::new(
            self.window_surface_geometry_loc(source).to_f64(),
            self.window_geometry_size(source).to_f64(),
        );
        let rect = rect
            .intersection(geometry)
            .ok_or("Application menu anchor is outside its source")?;
        let origin = if self.windows[source].kind == ManagedWindowKind::ShellBar {
            self.shell_bar_logical_origin(source)
        } else {
            self.launcher_logical_origin(source)
        }
        .ok_or("Application menu source has no placement")?;
        let scale = self
            .layout_backing_scale(source)
            .unwrap_or(self.window_projection(source).scale.into());
        let projected = Rectangle::new(
            origin.to_f64() + rect.loc.upscale(scale),
            rect.size.upscale(scale),
        )
        .to_i32_round();
        let clipped = self
            .layout_window_clip(source)
            .map_or(Some(projected), |clip| projected.intersection(clip))
            .and_then(|rect| rect.intersection(output))
            .ok_or("Application menu anchor is not visible")?;
        Ok(clipped.loc + Point::from((0, clipped.size.h)))
    }

    pub(in crate::compositor) fn desktop_context_source_available(&self) -> bool {
        self.desktop.context.source_window.is_none_or(|id| {
            self.windows.iter().enumerate().any(|(index, window)| {
                window.id == id && window.mapped && self.app_context_source_kind(index)
            })
        })
    }

    pub(in crate::compositor) fn desktop_context_takes_escape(&self) -> bool {
        if self.session_locked() {
            return false;
        }
        if self.desktop_context_pending() {
            return true;
        }
        let Some(context) = &self.desktop.context.current else {
            return false;
        };
        self.desktop.context.source_window.is_some()
            && self.keyboard.current_focus().is_some_and(|focus| {
                self.window_index_for_surface(focus.surface())
                    .is_some_and(|index| {
                        self.desktop.context.source_window == Some(self.windows[index].id)
                            || window_app_id(&self.windows[index].surface)
                                .as_deref()
                                .and_then(desktop_context::identity)
                                .is_some_and(|(generation, _)| generation == context.generation)
                    })
            })
    }

    pub(in crate::compositor) fn app_context_keeps_source(
        &self,
        panel: usize,
        focus: usize,
    ) -> bool {
        let Some(context) = &self.desktop.context.current else {
            return false;
        };
        let is_source =
            |index: usize| self.desktop.context.source_window == Some(self.windows[index].id);
        let is_menu = |index: usize| {
            window_app_id(&self.windows[index].surface)
                .as_deref()
                .and_then(desktop_context::identity)
                .is_some_and(|(generation, _)| generation == context.generation)
        };
        (is_source(panel) && is_menu(focus)) || (is_menu(panel) && is_source(focus))
    }

    pub(super) fn restore_app_context_source(&mut self, id: u64) -> bool {
        let Some(index) = self.windows.iter().position(|w| w.id == id && w.mapped) else {
            return false;
        };
        if !self.navigation_available() || !self.app_context_source_kind(index) {
            return false;
        }
        let Some(surface) = self.windows[index].surface.wl_surface() else {
            return false;
        };
        self.set_keyboard_focus_to_window(index, surface);
        true
    }
}
