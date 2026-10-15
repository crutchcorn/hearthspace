//! A real GTK editor for clipboard acceptance tests on either display backend.
//!
//! Clipboard operations use GtkTextView's ordinary Ctrl+A/C/X/V bindings, except
//! large X11 copies use a byte provider to avoid an upstream GTK serializer bug.
//! The input file seeds editor content (F5 reloads it); the output file reports
//! what the widget actually contains after an asynchronous paste completes.

use std::{cell::Cell, path::PathBuf, rc::Rc};

use gtk::{Application, ApplicationWindow, TextBuffer, TextView, gdk, gio, glib, prelude::*};
use serde::Serialize;

struct Options {
    title: String,
    input: PathBuf,
    output: PathBuf,
}

impl Options {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut title = None;
        let mut input = None;
        let mut output = None;
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let value = args.next().ok_or("each argument requires a value")?;
            match flag.as_str() {
                "--title" if title.is_none() => title = Some(value),
                "--input-file" if input.is_none() => input = Some(PathBuf::from(value)),
                "--output-file" if output.is_none() => output = Some(PathBuf::from(value)),
                _ => return Err(format!("unknown or repeated argument: {flag}").into()),
            }
        }
        Ok(Self {
            title: title.ok_or("--title is required")?,
            input: input.ok_or("--input-file is required")?,
            output: output.ok_or("--output-file is required")?,
        })
    }
}

#[derive(Serialize)]
struct Snapshot<'a> {
    backend: &'a str,
    text: String,
    revision: u64,
    clipboard_revision: u64,
    clipboard_serialization: &'a str,
    paste_completed: u64,
    ready: bool,
    focused: bool,
    pid: u32,
}

struct Recorder {
    output: PathBuf,
    buffer: TextBuffer,
    editor: TextView,
    window: ApplicationWindow,
    backend: &'static str,
    revision: Cell<u64>,
    clipboard_revision: Cell<u64>,
    clipboard_serialization: Cell<&'static str>,
    paste_completed: Cell<u64>,
}

impl Recorder {
    fn write(&self) {
        let snapshot = Snapshot {
            backend: self.backend,
            text: self
                .buffer
                .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
                .to_string(),
            revision: self.revision.get(),
            clipboard_revision: self.clipboard_revision.get(),
            clipboard_serialization: self.clipboard_serialization.get(),
            paste_completed: self.paste_completed.get(),
            ready: self.window.is_mapped(),
            focused: self.editor.has_focus(),
            pid: std::process::id(),
        };
        // Readers must never observe half a UTF-8 string or a partial JSON file.
        let temporary = self.output.with_extension("json.tmp");
        let contents = serde_json::to_vec(&snapshot).expect("serialize editor state");
        std::fs::write(&temporary, contents).expect("write editor state");
        std::fs::rename(temporary, &self.output).expect("publish editor state");
    }

    fn copy_large_x11_selection(&self) -> bool {
        if self.backend != "x11" {
            return false;
        }
        let Some((start, end)) = self.buffer.selection_bounds() else {
            return false;
        };
        let text = self.buffer.text(&start, &end, true);
        if text.len() <= 64 * 1024 {
            return false;
        }
        // GTK 4.22's string serializer can deadlock its event thread during INCR.
        // Fixed upstream: GNOME/gtk@420845f7bd4ede96d72d175d159110a5b48181af.
        // A MIME byte provider skips that serializer while retaining real GDK
        // X11 ownership, INCR transport, and ordinary destination paste handling.
        let bytes = glib::Bytes::from_owned(text.as_bytes().to_vec());
        let provider = gdk::ContentProvider::for_bytes("text/plain;charset=utf-8", &bytes);
        self.clipboard_serialization.set("gdk-utf8-bytes");
        self.editor
            .clipboard()
            .set_content(Some(&provider))
            .expect("offer large X11 clipboard selection");
        self.write();
        true
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Rc::new(Options::parse()?);
    let initial_text = std::fs::read_to_string(&options.input)?;
    let application = Application::builder()
        .application_id("dev.hearthspace.ClipboardTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(move |application| {
        build_window(application, &options, &initial_text);
    });
    application.run_with_args(&["hearthspace-clipboard-test"]);
    Ok(())
}

fn build_window(application: &Application, options: &Options, initial_text: &str) {
    let window = ApplicationWindow::builder()
        .application(application)
        .title(&options.title)
        .default_width(560)
        .default_height(320)
        .build();
    let editor = TextView::builder()
        .monospace(true)
        .left_margin(16)
        .right_margin(16)
        .top_margin(16)
        .bottom_margin(16)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let buffer = editor.buffer();
    buffer.set_text(initial_text);
    let scroller = gtk::ScrolledWindow::builder().child(&editor).build();
    window.set_child(Some(&scroller));
    let backend = match gtk::prelude::WidgetExt::display(&window).type_().name() {
        "GdkWaylandDisplay" => "wayland",
        "GdkX11Display" => "x11",
        other => panic!("unsupported test display: {other}"),
    };
    let recorder = Rc::new(Recorder {
        output: options.output.clone(),
        buffer: buffer.clone(),
        editor: editor.clone(),
        window: window.clone(),
        backend,
        revision: Cell::new(0),
        clipboard_revision: Cell::new(0),
        clipboard_serialization: Cell::new("none"),
        paste_completed: Cell::new(0),
    });
    let changed = Rc::clone(&recorder);
    buffer.connect_changed(move |_| {
        changed.revision.set(changed.revision.get() + 1);
        changed.write();
    });
    let pasted = Rc::clone(&recorder);
    buffer.connect_paste_done(move |_, _| {
        pasted.paste_completed.set(pasted.paste_completed.get() + 1);
        pasted.write();
    });
    let clipboard = Rc::clone(&recorder);
    editor.clipboard().connect_changed(move |_| {
        clipboard
            .clipboard_revision
            .set(clipboard.clipboard_revision.get() + 1);
        clipboard.write();
    });
    let mapped = Rc::clone(&recorder);
    window.connect_map(move |_| mapped.write());
    let focused = Rc::clone(&recorder);
    editor.connect_has_focus_notify(move |_| focused.write());

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let input = options.input.clone();
    let copied = Rc::clone(&recorder);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gdk::Key::F5 {
            buffer.set_text(&std::fs::read_to_string(&input).expect("reload input file"));
            return glib::Propagation::Stop;
        }
        if modifiers & gtk::accelerator_get_default_mod_mask() == gdk::ModifierType::CONTROL_MASK
            && matches!(key, gdk::Key::c | gdk::Key::C | gdk::Key::x | gdk::Key::X)
        {
            copied.clipboard_serialization.set("gtk-text-view");
            if matches!(key, gdk::Key::c | gdk::Key::C) && copied.copy_large_x11_selection() {
                return glib::Propagation::Stop;
            }
            copied.write();
        }
        glib::Propagation::Proceed
    });
    editor.add_controller(keys);
    window.present();
    editor.grab_focus();
    recorder.write();
}
