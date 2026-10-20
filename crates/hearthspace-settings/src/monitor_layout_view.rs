use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::{
    actions::{MonitorEditorAction, apply_monitor_editor_action},
    monitor_layout_widget::MonitorLayoutWidget,
    state::SettingsApp,
};

pub(crate) fn monitor_layout_editor() -> MonitorLayoutEditor {
    MonitorLayoutEditor
}

pub(crate) struct MonitorLayoutEditor;

impl ViewMarker for MonitorLayoutEditor {}

impl View<SettingsApp, (), ViewCtx> for MonitorLayoutEditor {
    type Element = Pod<MonitorLayoutWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(MonitorLayoutWidget::new(
                    state.settings.monitors.clone(),
                    state.selected_monitor_id.clone().or_else(|| {
                        state
                            .settings
                            .monitors
                            .first()
                            .map(|monitor| monitor.id.clone())
                    }),
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        element.widget.set_monitors(
            state.settings.monitors.clone(),
            state.selected_monitor_id.clone().or_else(|| {
                state
                    .settings
                    .monitors
                    .first()
                    .map(|monitor| monitor.id.clone())
            }),
            &mut element.ctx,
        );
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        let Some(action) = message.take_message::<MonitorEditorAction>() else {
            return MessageResult::Stale;
        };
        apply_monitor_editor_action(state, *action);
        MessageResult::Action(())
    }
}
