//! Native inspector hints are passive and can never outlive their editing session.
use super::{App, Rect, window_app_id};
use hearthspace_common::frame_layout::{layout_editor_from_app_id, layout_tooltip_from_app_id};
impl App {
    pub(in crate::compositor) fn is_layout_tooltip(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(layout_tooltip_from_app_id)
            .is_some()
    }
    pub(super) fn layout_tooltip_rect(&self, index: usize) -> Option<Rect> {
        let id = window_app_id(&self.windows[index].surface)?;
        let (session, output) = layout_tooltip_from_app_id(&id)?;
        let active = self.layout_editor.session.as_ref().filter(|s| {
            s.editing && s.id == session && output.as_ref().is_none_or(|o| *o == s.output)
        })?;
        let editor = self.windows.iter().enumerate().find_map(|(i, w)| {
            let identity = window_app_id(&w.surface)
                .as_deref()
                .and_then(layout_editor_from_app_id)?;
            (w.mapped && identity.0 == session).then_some(i)
        })?;
        let rect = self.layout_target_rect(editor)?;
        let view = self.frame_output_rect(&active.output)?;
        let size = self.window_geometry_size(index);
        let width = f64::from(size.w).min((view.width - 24.).max(1.));
        let height = f64::from(size.h);
        Some(Rect {
            x: (rect.x + (rect.width - width) / 2.).clamp(
                view.x + 12.,
                (view.x + view.width - width - 12.).max(view.x + 12.),
            ),
            y: (rect.y + rect.height + 8.)
                .min((view.y + view.height - height - 12.).max(view.y + 12.)),
            width,
            height,
        })
    }
}
