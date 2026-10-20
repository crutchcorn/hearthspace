use super::state::ShellState;
use masonry::{
    accesskit::{Action, ActionRequest, Role, TreeId},
    core::{Widget, WidgetId},
    testing::TestHarness,
};
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

pub(super) struct Ui<V: WidgetView<ShellState>>
where
    V::Widget: Sized,
{
    view: V,
    view_state: V::ViewState,
    pub(super) state: ShellState,
    ctx: ViewCtx,
    pub(super) harness: TestHarness<V::Widget>,
}

impl<V: WidgetView<ShellState>> Ui<V>
where
    V::Widget: Sized,
{
    pub(super) fn new(
        mut state: ShellState,
        build: impl Fn(&ShellState) -> V,
        size: (u32, u32),
    ) -> Self {
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let view = build(&state);
        let (root, view_state) = view.build(&mut ctx, &mut state);
        let harness = TestHarness::create_with_size(
            crate::ui::appearance::properties(state.appearance.palette),
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

    pub(super) fn rebuild(&mut self, build: &impl Fn(&ShellState) -> V) {
        self.harness
            .set_default_properties(self.state.appearance.properties.clone());
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

    pub(super) fn dispatch(&mut self, build: &impl Fn(&ShellState) -> V) {
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

    pub(super) fn activate(&mut self, id: WidgetId) {
        self.harness.process_access_event(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: id.to_raw().into(),
            data: None,
        });
    }

    pub(super) fn button(&self, name: &str) -> WidgetId {
        ids(&self.harness)
            .into_iter()
            .find(|id| {
                self.harness.access_node(*id).is_some_and(|node| {
                    node.role() == Role::Button && node.label().as_deref() == Some(name)
                })
            })
            .unwrap_or_else(|| panic!("Missing accessible button: {name}"))
    }
}

pub(super) fn ids<W: Widget>(harness: &TestHarness<W>) -> Vec<WidgetId> {
    let mut result = vec![];
    let mut pending = vec![harness.root_id()];
    while let Some(id) = pending.pop() {
        pending.extend(
            harness
                .get_widget_with_id(id)
                .children()
                .iter()
                .map(|child| child.id()),
        );
        result.push(id);
    }
    result
}
