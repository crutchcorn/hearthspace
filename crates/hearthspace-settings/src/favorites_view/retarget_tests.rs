use super::tests::{draw, state, view};
use crate::test_support::Ui;
use hearthspace_desktop::favorites::{FavoriteStore, Recognition};
use masonry::{accesskit::Role, core::pointer::PointerButton};

#[test]
fn retargeting_through_settings_keeps_both_pins_in_their_saved_order() {
    let (dir, mut state) = state();
    let store = state.favorites.store.as_mut().unwrap();
    store.add("Beta.desktop").unwrap();
    store
        .set_gesture("Alpha.desktop", &crate::favorites::tests::shape())
        .unwrap();
    let path = dir.path().join("favorites.json");
    let before = std::fs::read(&path).unwrap();
    let mut ui = Ui::new(state, view, (760, 700));
    ui.activate(ui.button("Change gesture for Alpha"));
    ui.dispatch(&view);
    draw(&mut ui, &crate::favorites::tests::shape());
    ui.dispatch(&view);
    let points = ui
        .state
        .favorites
        .recording
        .as_ref()
        .unwrap()
        .points
        .clone();
    ui.activate(ui.button("Choose app"));
    ui.dispatch(&view);
    // Popup options occupy an attached layer, outside the main widget subtree.
    let beta = {
        let mut pending = vec![ui.harness.access_tree().state().root()];
        loop {
            let node = pending.pop().expect("missing Beta picker option");
            pending.extend(node.children());
            if node.role() == Role::ListBoxOption && node.label().as_deref() == Some("Beta") {
                break node.bounding_box().expect("Beta picker option bounds");
            }
        }
    };
    ui.harness
        .mouse_move(((beta.x0 + beta.x1) / 2.0, (beta.y0 + beta.y1) / 2.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(
        ui.state.favorites.recording.as_ref().unwrap().app_id,
        "Beta.desktop"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    ui.activate(ui.button("Save shortcut"));
    ui.dispatch(&view);
    ui.button("Change gesture for Beta");
    assert!(ui.state.favorites.recording.is_none());
    let saved = FavoriteStore::load(path).unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .map(|entry| entry.app_id.as_str())
            .collect::<Vec<_>>(),
        ["Beta.desktop", "Alpha.desktop"]
    );
    assert!(saved.entries()[1].gesture.is_none());
    assert_eq!(
        saved.recognize(&points).unwrap(),
        Recognition::Match("Beta.desktop".into())
    );
    // JSON round trips can change the last bits of normalized coordinates;
    // pin membership and recognition must agree with the live Settings store.
    let live = ui.state.favorites.store.as_ref().unwrap();
    assert_eq!(
        live.entries()
            .iter()
            .map(|entry| (entry.app_id.as_str(), entry.gesture.is_some()))
            .collect::<Vec<_>>(),
        [("Beta.desktop", true), ("Alpha.desktop", false)]
    );
    assert_eq!(
        live.recognize(&points).unwrap(),
        Recognition::Match("Beta.desktop".into())
    );
}
