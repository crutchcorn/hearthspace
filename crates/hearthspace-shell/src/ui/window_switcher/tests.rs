use super::*;
use crate::ui::test_support::{Ui, ids};
use masonry::accesskit::{Action, Live, Role};
use xilem::{WidgetView, view::sized_box};

fn prepared(session: u64, title: &str) -> Prepared {
    let (displayed, width) = text::Metrics::default().prepare(title, 240.0);
    Prepared {
        snapshot: Snapshot {
            version: 1,
            revision: 4,
            active: true,
            session,
            title: title.into(),
            index: 2,
            count: 3,
            caption: Bounds {
                width,
                height: 24.0,
                max_width: 240.0,
                ..Default::default()
            },
        },
        displayed,
    }
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(caption::view(state))
}

#[test]
fn selected_title_announces_position_without_taking_keyboard_focus() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    let title = "Résumé – an extremely long document title that must remain fully accessible";
    apply(&mut state, prepared(7, title));
    assert!(state.window_switcher.prepared.displayed.ends_with('…'));
    let mut ui = Ui::new(state, view, (240, 24));
    assert_eq!(ui.harness.focused_widget_id(), None);
    let status = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|node| node.role() == Role::Status)
        })
        .unwrap();
    let node = ui.harness.access_node(status).unwrap();
    assert_eq!(
        node.label().as_deref(),
        Some(format!("{title}, 2 of 3").as_str())
    );
    assert_eq!(node.live(), Live::Polite);
    assert!(!node.data().supports_action(Action::Focus));
    apply(&mut ui.state, prepared(7, "Terminal"));
    ui.rebuild(&view);
    ui.harness.redraw();
    assert_eq!(
        ui.harness.access_node(status).unwrap().label().as_deref(),
        Some("Terminal, 2 of 3")
    );
    assert_eq!(ui.harness.focused_widget_id(), None);
}

#[test]
fn dismissal_drops_private_caption_and_reopening_uses_a_fresh_surface() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    assert!(window(&state).is_none());
    apply(&mut state, prepared(7, "Private title"));
    let first = state.window_switcher.window_id;
    assert!(window(&state).is_some());
    apply(&mut state, prepared(7, "Another window"));
    assert_eq!(state.window_switcher.window_id, first);
    apply(&mut state, Prepared::default());
    assert!(window(&state).is_none());
    assert!(state.window_switcher.prepared.snapshot.title.is_empty());
    apply(&mut state, prepared(8, "New session"));
    assert_ne!(state.window_switcher.window_id, first);
}

#[test]
fn intrinsic_caption_width_and_unicode_ellipsis_fit_the_selected_preview() {
    let mut metrics = text::Metrics::default();
    let (short, width) = metrics.prepare("Notes", 300.0);
    assert_eq!(short, "Notes");
    assert!((26.0..300.0).contains(&width));
    let (long, width) = metrics.prepare(&"Résumé 🖥 — ".repeat(100), 120.0);
    assert_eq!(width, 120.0);
    assert!(long.ends_with('…'));
    assert_eq!(metrics.prepare(&long, 120.0).0, long);
    assert_eq!(metrics.prepare("Too narrow", 5.0), (String::new(), 5.0));
}
