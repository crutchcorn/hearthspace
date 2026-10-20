//! A bounded single-stroke drawing surface. Recognition and launching stay in
//! the application; this view only collects intentional pointer input.

use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
};

mod widget;
use widget::{DrawingAction, DrawingPad};
mod thumbnail;
pub use thumbnail::{thumbnail, thumbnail_sized};

pub struct GesturePad<State> {
    pub output_key: String,
    /// Change when entering another mode/recording target to cancel old input.
    pub context: u64,
    /// Settings records locally; the shell receives compositor-guarded strokes.
    pub capture_locally: bool,
    pub status: String,
    /// Live externally collected stroke, in local drawing-area coordinates.
    pub preview: Vec<[f64; 2]>,
    pub on_stroke: fn(&mut State, &str, u64, Vec<[f64; 2]>),
    /// Logical client-space x, y, width, height; emitted after layout settles.
    pub on_bounds: fn(&mut State, &str, u64, [f64; 4]),
}

impl<State> ViewMarker for GesturePad<State> {}

impl<State: 'static> View<State, (), ViewCtx> for GesturePad<State> {
    type Element = Pod<DrawingPad>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(DrawingPad::new(
                    self.context,
                    self.capture_locally,
                    self.status.clone(),
                    self.preview.clone(),
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        old: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.context != old.context
            || self.output_key != old.output_key
            || self.capture_locally != old.capture_locally
        {
            element.widget.context = self.context;
            element.widget.capture_locally = self.capture_locally;
            element.widget.stroke = None;
            element.widget.last_bounds = None;
        }
        if self.status != old.status
            || self.preview != old.preview
            || self.context != old.context
            || self.output_key != old.output_key
            || self.capture_locally != old.capture_locally
        {
            element.widget.status = self.status.clone();
            element.widget.set_preview(&self.preview);
            element.ctx.request_render();
            element.ctx.request_accessibility_update();
        }
        // Parent content can move the pad without changing its own size.
        element.ctx.request_compose();
    }

    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        let Some(action) = message.take_message::<DrawingAction>() else {
            return MessageResult::Stale;
        };
        match *action {
            DrawingAction::Stroke { context, points } if context == self.context => {
                (self.on_stroke)(state, &self.output_key, context, points);
            }
            DrawingAction::Bounds { context, rect } if context == self.context => {
                (self.on_bounds)(state, &self.output_key, context, rect);
            }
            // A queued action from a previous recording or launch mode must
            // never be interpreted against its replacement.
            _ => {}
        }
        MessageResult::Action(())
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
