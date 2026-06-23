use gtk::{
    Align, Application, ApplicationWindow, Button, CheckButton, Entry, Label, ListBox, ListBoxRow,
    Orientation, prelude::*,
};

use crate::config::{GTK_TEST_APP_ID, GTK_TEST_APP_TITLE};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app = Application::builder()
        .application_id(GTK_TEST_APP_ID)
        .build();

    app.connect_activate(build_window);
    app.run_with_args(&["hearthspace-gtk-test-app"]);

    Ok(())
}

fn build_window(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title(GTK_TEST_APP_TITLE)
        .default_width(520)
        .default_height(420)
        .build();

    let root = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let heading = Label::new(Some("Research Workspace"));
    heading.set_halign(Align::Start);
    heading.add_css_class("title-1");
    root.append(&heading);

    let summary = Label::new(Some(
        "A deterministic GTK test app for Hearthspace semantic-tree and MCP experiments.",
    ));
    summary.set_halign(Align::Start);
    summary.set_wrap(true);
    root.append(&summary);

    let query = Entry::builder()
        .placeholder_text("Research query")
        .text("Wayland compositor accessibility")
        .build();
    root.append(&query);

    let include_summaries = CheckButton::with_label("Include paper summaries");
    include_summaries.set_active(true);
    root.append(&include_summaries);

    let actions = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    actions.append(&Button::with_label("Open research notes"));
    actions.append(&Button::with_label("Center research windows"));
    actions.append(&Button::with_label("Summarize active paper"));
    root.append(&actions);

    let list_label = Label::new(Some("Research items"));
    list_label.set_halign(Align::Start);
    list_label.add_css_class("heading");
    root.append(&list_label);

    let list = ListBox::new();
    for item in [
        "Paper: AT-SPI object trees on Wayland",
        "Note: Match Hearthspace window IDs to semantic roots",
        "Task: Move all research windows to screen center",
        "Reference: Model Context Protocol tool chain",
    ] {
        let row = ListBoxRow::new();
        let label = Label::new(Some(item));
        label.set_halign(Align::Start);
        label.set_margin_top(6);
        label.set_margin_bottom(6);
        label.set_margin_start(8);
        label.set_margin_end(8);
        row.set_child(Some(&label));
        list.append(&row);
    }
    root.append(&list);

    window.set_child(Some(&root));
    window.present();
}
