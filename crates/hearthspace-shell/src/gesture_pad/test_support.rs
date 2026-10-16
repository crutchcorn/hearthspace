use hearthspace_ui_components as components;
use masonry::testing::TestHarness;
use std::sync::Arc;
use xilem::{
    ViewCtx, WidgetView,
    core::{DynMessage, MessageCtx, MessageResult, RawProxy, ViewPathTracker},
};

#[derive(Debug)]
struct NoMessages;

impl RawProxy for NoMessages {
    fn send_message(
        &self,
        _path: Arc<[xilem::core::ViewId]>,
        _message: xilem::core::SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("map views must not start background tasks")
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

pub(super) struct Ui<State: 'static, V: WidgetView<State>>
where
    V::Widget: Sized,
{
    view: V,
    view_state: V::ViewState,
    pub(super) state: State,
    ctx: ViewCtx,
    pub(super) harness: TestHarness<V::Widget>,
}

impl<State: 'static, V: WidgetView<State>> Ui<State, V>
where
    V::Widget: Sized,
{
    pub(super) fn new(mut state: State, build: impl Fn(&State) -> V, size: (u32, u32)) -> Self {
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let view = build(&state);
        let (root, view_state) = view.build(&mut ctx, &mut state);
        let harness = TestHarness::create_with_size(
            components::hearthspace_light_properties(),
            root.new_widget,
            size,
        );
        let mut ui = Self {
            view,
            view_state,
            state,
            ctx,
            harness,
        };
        ui.dispatch(&build);
        ui
    }

    pub(super) fn rebuild(&mut self, build: &impl Fn(&State) -> V) {
        let next = build(&self.state);
        self.harness.edit_root_widget(|root| {
            next.rebuild(
                &self.view,
                &mut self.view_state,
                &mut self.ctx,
                root,
                &mut self.state,
            );
        });
        self.view = next;
    }

    pub(super) fn dispatch(&mut self, build: &impl Fn(&State) -> V) {
        while let Some((action, id)) = self.harness.pop_action_erased() {
            let path = self.ctx.get_id_path(id).unwrap().clone();
            let mut message = MessageCtx::new(
                std::mem::take(self.ctx.environment()),
                path,
                DynMessage(action),
            );
            let result = self.harness.edit_root_widget(|root| {
                self.view
                    .message(&mut self.view_state, &mut message, root, &mut self.state)
            });
            assert!(matches!(
                result,
                MessageResult::Action(()) | MessageResult::RequestRebuild
            ));
            let (environment, _, _) = message.finish();
            *self.ctx.environment() = environment;
            self.rebuild(build);
        }
        self.harness.redraw();
    }
}
