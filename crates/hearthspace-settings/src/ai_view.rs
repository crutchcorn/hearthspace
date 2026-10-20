use hearthspace_ai::DevicePreference;
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, progress_bar, sized_box},
};

use crate::{
    ai::Operation,
    page_components::{card, note, section},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let ai = &state.ai;
    let busy = ai.pending.is_some();
    let installed = ai.installed();
    let device = ai.device();
    section(
        "Gather similar windows",
        card(flex_col((
            note("Select windows on the canvas, then use Gather similar windows to find related open windows by their titles. Titles are processed locally."),
            components::field_label("LiquidAI D1 · 3B · Q4_K_M"),
            note("Optional download from Hugging Face. Requires Ollama with D1 decision support. Text only, with a 32K context window. Memory use depends on context and processing mode."),
            components::field_label(match &ai.snapshot {
                None => "Model status unknown",
                Some(status) if status.installed => "Model installed",
                Some(status) if status.downloaded => "Model download present; runtime setup incomplete",
                Some(_) => "Model not installed",
            }),
            note(ai.message.clone()),
            busy.then(|| sized_box(progress_bar(ai.progress)).fixed_width(280.0.px())),
            flex_row((
                button(label("Download D1 model").text_size(13.0), |state: &mut SettingsApp| {
                    state.ai.request(Operation::Install);
                }).disabled(busy || installed),
                button(label("Delete D1 model").text_size(13.0), |state: &mut SettingsApp| {
                    state.ai.request(Operation::Remove);
                }).disabled(busy || !ai.can_remove()),
            )).gap(8.0.px()),
            button(label("Refresh status").text_size(13.0), |state: &mut SettingsApp| {
                state.ai.request(Operation::Refresh);
            }).disabled(busy),
            installed.then(|| button(label("Load D1 model").text_size(13.0), |state: &mut SettingsApp| {
                state.ai.request(Operation::Warm);
            }).disabled(busy || ai.snapshot.as_ref().is_some_and(|status| status.warm))),
            components::field_label("Processing mode"),
            flex_row((
                components::button_surface(button(label("Automatic (GPU if available)").text_size(13.0), |state: &mut SettingsApp| {
                    state.ai.request(Operation::SetDevice(DevicePreference::Automatic));
                }).disabled(busy || device == DevicePreference::Automatic)
                    , components::ButtonSurface::Control, device == DevicePreference::Automatic),
                components::button_surface(button(label("CPU only").text_size(13.0), |state: &mut SettingsApp| {
                    state.ai.request(Operation::SetDevice(DevicePreference::Cpu));
                }).disabled(busy || device == DevicePreference::Cpu)
                    , components::ButtonSurface::Control, device == DevicePreference::Cpu),
            )).gap(8.0.px()),
            note(if device == DevicePreference::Automatic { "Current mode: Automatic" } else { "Current mode: CPU only" }),
            note("When installed, Hearthspace loads D1 after login and keeps it ready for this session. Deleting the model releases its memory and removes the managed download."),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px())),
    )
    .boxed()
}

#[cfg(test)]
mod tests;
