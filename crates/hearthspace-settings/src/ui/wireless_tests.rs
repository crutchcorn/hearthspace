use crate::test_support::Ui;
use hearthspace_desktop::wireless::{Command, Item, Radio, state::Update};
#[test]
fn details_navigation_is_local_and_a_removed_network_cannot_rebind_the_inspector() {
    let (_dir, mut s) = super::tests::state();
    s.wireless.snapshot.wifi = Ok(Radio {
        enabled: true,
        items: vec![Item {
            id: "first".into(),
            name: "Studio".into(),
            detail: "Connected".into(),
            connected: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    let (tx, mut rx) = xilem::tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let build =
        |s: &crate::state::SettingsApp| xilem::view::sized_box(crate::wireless::panel(s, false));
    let mut ui = Ui::new(s, build, (320, 400));
    ui.activate(ui.button("Studio details"));
    ui.dispatch(&build);
    assert_eq!(ui.state.wireless.details.as_deref(), Some("first"));
    assert!(
        rx.try_recv().is_err(),
        "opening details must not mutate radios"
    );
    let back = ui.button("Back");
    ui.activate(back);
    ui.dispatch(&build);
    assert!(ui.state.wireless.details.is_none());
    ui.activate(ui.button("Studio details"));
    ui.dispatch(&build);
    let mut snapshot = ui.state.wireless.snapshot.clone();
    snapshot.wifi.as_mut().unwrap().items[0].id = "replacement".into();
    ui.state.wireless.receive(Update {
        snapshot,
        finished: false,
        error: None,
    });
    ui.rebuild(&build);
    ui.dispatch(&build);
    assert!(ui.state.wireless.details.is_none());
    assert!(rx.try_recv().is_err());
    ui.activate(ui.button("Disconnect from Studio"));
    ui.dispatch(&build);
    assert!(matches!(rx.try_recv(),Ok(Command::DisconnectWifi(id)) if id=="replacement"));
}
