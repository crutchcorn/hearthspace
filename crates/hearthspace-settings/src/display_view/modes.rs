use super::*;
use hearthspace_ipc::display_modes::{Mode, Request};

fn refresh_label(mode: &Mode) -> String {
    let rate = f64::from(mode.refresh_millihertz) / 1000.;
    if mode.refresh_millihertz % 1000 == 0 {
        format!("{rate:.0} Hz")
    } else {
        format!("{rate:.3} Hz")
    }
}

type Field = Box<AnyWidgetView<SettingsApp>>;

pub(super) fn controls(state: &SettingsApp, id: &str) -> Option<(Field, Field)> {
    let modes = &state.display_layout.modes;
    let output = modes.snapshot.outputs.iter().find(|o| o.id == id)?;
    let disabled = !modes.snapshot.available || modes.busy || modes.snapshot.preview.is_some();
    let mut resolutions: Vec<_> = output.modes.iter().map(|m| (m.width, m.height)).collect();
    resolutions.sort_unstable();
    resolutions.dedup();
    resolutions.reverse();
    let current = &output.current;
    let resolution = resolutions
        .iter()
        .position(|&(w, h)| w == current.width && h == current.height)?;
    let candidates: Vec<_> = resolutions
        .iter()
        .filter_map(|&(w, h)| {
            output
                .modes
                .iter()
                .filter(|m| m.width == w && m.height == h)
                .min_by_key(|m| {
                    (
                        (m.refresh_millihertz - current.refresh_millihertz).abs(),
                        m.id != current.id,
                    )
                })
                .cloned()
        })
        .collect();
    let output_id = id.to_owned();
    let resolution = select(
        state.palette(),
        "Resolution",
        resolutions
            .iter()
            .map(|(w, h)| format!("{w} × {h}"))
            .collect(),
        resolution,
        disabled,
        move |state, index| {
            crate::display_modes::request(
                state,
                Request::Preview {
                    output: output_id.clone(),
                    mode: candidates[index].id.clone(),
                },
            );
        },
    );
    let mut rates: Vec<_> = output
        .modes
        .iter()
        .filter(|m| m.width == current.width && m.height == current.height)
        .cloned()
        .collect();
    // Equivalent timing entries need not appear twice in a refresh selector.
    rates.sort_by_key(|m| (std::cmp::Reverse(m.refresh_millihertz), m.id != current.id));
    rates.dedup_by_key(|m| m.refresh_millihertz);
    let selected = rates
        .iter()
        .position(|m| m.refresh_millihertz == current.refresh_millihertz)?;
    let labels = rates.iter().map(refresh_label).collect();
    let output_id = id.to_owned();
    let refresh = select(
        state.palette(),
        "Refresh rate",
        labels,
        selected,
        disabled,
        move |state, index| {
            crate::display_modes::request(
                state,
                Request::Preview {
                    output: output_id.clone(),
                    mode: rates[index].id.clone(),
                },
            );
        },
    );
    Some((resolution.boxed(), refresh.boxed()))
}

pub(super) fn confirmation(state: &SettingsApp) -> Option<Box<AnyWidgetView<SettingsApp>>> {
    use components::controls::{ButtonVariant, form_button};
    let modes = &state.display_layout.modes;
    let Some(preview) = &modes.snapshot.preview else {
        let message = modes.error.clone().or(modes.snapshot.message.clone())?;
        return Some(note(state.palette(), message).boxed());
    };
    let token = preview.token;
    Some(flex_col((
        label("Keep these display settings?").text_size(16.0),
        note(state.palette(),if preview.ready {format!("Reverting in {} seconds unless you keep these settings.",preview.seconds_remaining)} else {"Waiting for the new mode to be presented… Changes revert automatically if it fails.".into()}),
        flex_row((
            crate::page_components::focus_on_mount(form_button(state.palette(),"Revert",ButtonVariant::Neutral,modes.busy,move |state|crate::display_modes::request(state,Request::Revert{token}))),
            form_button(state.palette(),"Keep changes",ButtonVariant::Primary,modes.busy || !preview.ready,move |state|crate::display_modes::request(state,Request::Keep{token})),
        )).gap(8.0.px()),
        note(state.palette(),modes.error.clone().unwrap_or_default()),
    )).cross_axis_alignment(CrossAxisAlignment::Stretch).gap(8.0.px()).boxed())
}
