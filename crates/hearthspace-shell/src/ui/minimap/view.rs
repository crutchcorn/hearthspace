use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
};

use super::{
    CanvasSnapshot,
    widget::{MapAction, Minimap},
};
use crate::{
    ShellCommand,
    ui::{command_client::send_command, state::ShellState},
};

pub(super) struct MinimapView {
    pub(super) snapshot: Option<CanvasSnapshot>,
    pub(super) output_name: Option<String>,
}

impl ViewMarker for MinimapView {}

impl View<ShellState, (), ViewCtx> for MinimapView {
    type Element = Pod<Minimap>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(Minimap::new(
                    self.snapshot.clone(),
                    self.output_name.clone(),
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.snapshot != prev.snapshot || self.output_name != prev.output_name {
            element.widget.snapshot = self.snapshot.clone();
            element.widget.output_name = self.output_name.clone();
            element.ctx.request_render();
            element.ctx.request_accessibility_update();
        }
    }

    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match message.take_message::<MapAction>().map(|action| *action) {
            Some(MapAction::Begin) => state.canvas_dragging = true,
            Some(MapAction::End) => state.canvas_dragging = false,
            Some(MapAction::PanTo { x, y }) => {
                match send_command(
                    &state.command_socket,
                    &ShellCommand::PanTo { x, y }.wire_name(),
                ) {
                    Ok(()) => {
                        state
                            .canvas_revision
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        if let Some(snapshot) = &mut state.canvas {
                            snapshot.set_offset(x, y);
                        }
                        state.canvas_error = None;
                    }
                    Err(error) => {
                        state.canvas_error = Some(error);
                        state.canvas_dragging = false;
                        element.widget.drag = None;
                        element.widget.snapshot = state.canvas.clone();
                        element.ctx.request_render();
                        element.ctx.request_cursor_icon_change();
                    }
                }
            }
            None => return MessageResult::Stale,
        }
        MessageResult::Action(())
    }
}
