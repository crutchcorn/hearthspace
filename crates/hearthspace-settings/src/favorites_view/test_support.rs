use crate::state::SettingsApp;
use hearthspace_ui_components as components;
use masonry::{
    accesskit::{Action, ActionRequest, Role, TreeId},
    core::{Widget, WidgetId},
    testing::{TestHarness, TestHarnessParams},
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

pub(crate) struct Ui<V: WidgetView<SettingsApp>>
where
    V::Widget: Sized,
{
    view: V,
    view_state: V::ViewState,
    pub(crate) state: SettingsApp,
    ctx: ViewCtx,
    pub(crate) harness: TestHarness<V::Widget>,
}

impl<V: WidgetView<SettingsApp>> Ui<V>
where
    V::Widget: Sized,
{
    pub(crate) fn new(
        mut state: SettingsApp,
        build: impl Fn(&SettingsApp) -> V,
        size: (u32, u32),
    ) -> Self {
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let view = build(&state);
        let (root, view_state) = view.build(&mut ctx, &mut state);
        let harness = TestHarness::create_with(
            components::hearthspace_opaque_properties(),
            root.new_widget,
            TestHarnessParams::default()
                .with_size(size)
                .with_background(xilem::Color::TRANSPARENT),
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

    pub(crate) fn rebuild(&mut self, build: &impl Fn(&SettingsApp) -> V) {
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

    pub(crate) fn dispatch(&mut self, build: &impl Fn(&SettingsApp) -> V) {
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

    pub(crate) fn activate(&mut self, id: WidgetId) {
        self.harness.process_access_event(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: id.to_raw().into(),
            data: None,
        });
    }

    pub(crate) fn button(&self, name: &str) -> WidgetId {
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

pub(crate) fn ids<W: Widget>(harness: &TestHarness<W>) -> Vec<WidgetId> {
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
