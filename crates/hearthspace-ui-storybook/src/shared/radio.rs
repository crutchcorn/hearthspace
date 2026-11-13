use super::*;
pub(super) fn preview(state: &AppState) -> Box<AnyWidgetView<AppState>> {
    let id = state.selected_story().id();
    let disabled = state.selected_bool("disabled").unwrap_or(false);
    let p = state.palette();
    let title = if id.ends_with("bluetooth") {
        "Bluetooth"
    } else {
        "Wi-Fi"
    };
    let model = components::radio_panel::Panel {
        title,
        enabled: Some(state.selected_bool("checked").unwrap_or(true)),
        busy: disabled,
        blocked: false,
        message: None,
        selected: state
            .selected_text("details")
            .filter(|id| !id.is_empty())
            .map(str::to_owned),
        rows: vec![components::radio_panel::Row {
            id: "sample".into(),
            name: "Example device".into(),
            detail: "Available".into(),
            connected: false,
            known: true,
            secured: false,
            strength: (title != "Bluetooth").then_some(60),
            actions: vec![("Connect".into(), ())],
        }],
    };
    if state.selected_bool("compact").unwrap_or(false) {
        return xilem::view::flex_col((
            components::radio_panel::header(
                title,
                model.enabled,
                disabled,
                |s: &mut AppState, on| s.set_selected_bool("checked", on),
            ),
            components::touch_portal(components::radio_panel::body(
                p,
                true,
                model,
                |s: &mut AppState| s.clicks += 1,
                |s: &mut AppState, ()| s.clicks += 1,
                |_: &mut AppState, _| {},
            ))
            .constrain_horizontal(true)
            .must_fill(false),
            components::radio_panel::settings_link(p, title == "Bluetooth", |s: &mut AppState| {
                s.clicks += 1
            }),
        ))
        .cross_axis_alignment(xilem::view::CrossAxisAlignment::Stretch)
        .gap(8.0.px())
        .boxed();
    }
    components::radio_panel::view(
        p,
        model,
        |s: &mut AppState, on| s.set_selected_bool("checked", on),
        |s: &mut AppState| s.clicks += 1,
        |s: &mut AppState, ()| s.clicks += 1,
        |s: &mut AppState, id| s.set_selected_text("details", id.unwrap_or_default()),
    )
    .boxed()
}
