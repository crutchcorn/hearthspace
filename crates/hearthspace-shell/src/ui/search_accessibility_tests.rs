use super::*;
use accesskit_atspi_common::{
    Adapter, AdapterCallback, AppContext, Event, InterfaceSet, NodeId, WindowBounds,
};
use masonry::{accesskit, core::Widget, testing::TestHarness};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct InterfaceState {
    registered: HashMap<NodeId, u32>,
    errors: Vec<String>,
}
#[derive(Clone, Default)]
struct Interfaces(Arc<Mutex<InterfaceState>>);
impl Interfaces {
    fn assert_balanced(&self) {
        let errors = self.0.lock().unwrap().errors.clone();
        assert!(errors.is_empty(), "{errors:?}");
    }
}
impl AdapterCallback for Interfaces {
    fn register_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut registered = self.0.lock().unwrap();
        *registered.registered.entry(id).or_default() |= interfaces.bits();
    }
    fn unregister_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut registered = self.0.lock().unwrap();
        let current = *registered.registered.entry(id).or_default();
        if current & interfaces.bits() != interfaces.bits() {
            registered.errors.push(format!(
                "removing missing interfaces for {id:?}: {interfaces:?}, current={current}"
            ));
        }
        registered
            .registered
            .insert(id, current & !interfaces.bits());
    }
    fn emit_event(&self, _: &Adapter, _: Event) {}
}
struct NoActions;
impl accesskit::ActionHandler for NoActions {
    fn do_action(&mut self, _: accesskit::ActionRequest) {}
}
fn snapshot<W: Widget>(harness: &TestHarness<W>) -> accesskit::TreeUpdate {
    let state = harness.access_tree().state();
    let mut nodes = vec![];
    let mut pending = vec![state.root()];
    while let Some(node) = pending.pop() {
        let id = ((u128::from(node.id()) >> 64) as u64).into();
        nodes.push((id, node.data().clone()));
        pending.extend(node.children());
    }
    accesskit::TreeUpdate {
        tree_id: accesskit::TreeId::ROOT,
        nodes,
        tree: Some(accesskit::Tree::new(
            ((u128::from(state.root_id()) >> 64) as u64).into(),
        )),
        focus: ((u128::from(state.focus_id_in_tree()) >> 64) as u64).into(),
    }
}

#[test]
fn search_result_removal_keeps_platform_interfaces_balanced() {
    let mut state = state();
    state.change_query("eDP-1", String::new());
    let mut ui = Ui::new(state, view, (360, 144));
    let interfaces = Interfaces::default();
    let mut adapter = Adapter::new(
        &AppContext::new(None),
        interfaces.clone(),
        snapshot(&ui.harness),
        true,
        WindowBounds::default(),
        NoActions,
    );
    for query in ["N", "No", "Notes", "Notes 2", "Notes", "", "Notes", ""] {
        ui.state.change_query("eDP-1", query.into());
        ui.rebuild(&view);
        ui.dispatch(&view);
        adapter.update(snapshot(&ui.harness));
        let height = crate::ui::popup::size(
            &ui.state.outputs[0],
            crate::ui::popup::PopupKind::Search,
            360.0,
            144.0,
        )
        .height as u32;
        ui.harness
            .process_window_event(masonry::core::WindowEvent::Resize((360, height).into()));
        ui.dispatch(&view);
        adapter.update(snapshot(&ui.harness));
        interfaces.assert_balanced();
    }
    drop(adapter);
    interfaces.assert_balanced();
}

#[test]
fn whole_panel_content_remains_accessible_through_growth_and_removal() {
    use crate::ui::popup::{self, PopupKind};
    use xilem::{
        masonry::layout::AsUnit,
        style::Style,
        view::{button, flex_col, label},
    };
    for kind in [PopupKind::Map, PopupKind::Notifications, PopupKind::Power] {
        let build = |state: &ShellState| {
            let rows = (0..state.outputs[0].query.len())
                .map(|index| {
                    button(label(format!("Action {index}")), |_: &mut ShellState| {})
                        .height(40.0.px())
                })
                .collect::<Vec<_>>();
            sized_box(popup::panel(&state.outputs[0], kind, flex_col(rows)))
        };
        let mut state = state();
        state.change_query("eDP-1", String::new());
        let mut ui = Ui::new(state, &build, (360, 144));
        let interfaces = Interfaces::default();
        let mut adapter = Adapter::new(
            &AppContext::new(None),
            interfaces.clone(),
            snapshot(&ui.harness),
            true,
            WindowBounds::default(),
            NoActions,
        );
        for query in ["abcdefgh", "a", "abcdefgh", ""] {
            ui.state.change_query("eDP-1", query.into());
            ui.rebuild(&build);
            ui.dispatch(&build);
            adapter.update(snapshot(&ui.harness));
            let height = popup::size(&ui.state.outputs[0], kind, 360.0, 144.0).height as u32;
            ui.harness
                .process_window_event(masonry::core::WindowEvent::Resize((360, height).into()));
            ui.dispatch(&build);
            adapter.update(snapshot(&ui.harness));
            interfaces.assert_balanced();
        }
        drop(adapter);
        interfaces.assert_balanced();
    }
}
