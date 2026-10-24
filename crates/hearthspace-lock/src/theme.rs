//! Lock-specific layout, using the same appearance tokens as the shell.
use gtk::gdk;
use hearthspace_ui_components::Palette;

pub(crate) struct Style {
    display: gdk::Display,
    provider: gtk::CssProvider,
}

impl Drop for Style {
    fn drop(&mut self) {
        gtk::style_context_remove_provider_for_display(&self.display, &self.provider);
    }
}

pub(crate) fn install(display: &gdk::Display, palette: Palette) -> Style {
    let p = palette;
    let css = include_str!("theme.css")
        .replace("@field@", &rgba(p.app_background))
        .replace("@ink@", &rgba(p.text))
        .replace("@muted@", &rgba(p.muted))
        .replace("@surface@", &rgba(p.chrome))
        .replace("@raised@", &rgba(p.chrome_raised))
        .replace("@border@", &rgba(p.chrome_border))
        .replace("@accent@", &rgba(p.accent))
        .replace("@accent-hover@", &rgba(p.accent_hover))
        .replace("@accent-ink@", &rgba(p.accent_ink))
        .replace("@strong@", &rgba(p.accent_decorative))
        .replace("@accent-border@", &rgba(p.accent_border))
        .replace("@control@", &rgba(p.surface_control))
        .replace("@hover@", &rgba(p.button_hover))
        .replace("@edge@", &rgba(p.glass_edge_top))
        .replace("@edge-bottom@", &rgba(p.glass_edge_bottom))
        .replace("@active-edge@", &rgba(p.glass_edge_active_top))
        .replace("@panel-shadow@", &rgba(p.panel_shadow))
        .replace("@accent-shadow@", &rgba(p.panel_accent_shadow))
        .replace("@glow@", &rgba(p.glow_shadow));
    let provider = gtk::CssProvider::new();
    provider.connect_parsing_error(|_, section, error| {
        eprintln!("lock appearance error at {section:?}: {error}");
    });
    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Style {
        display: display.clone(),
        provider,
    }
}

fn rgba(color: hearthspace_ui_components::Color) -> String {
    let c = color.to_rgba8();
    format!(
        "rgba({}, {}, {}, {})",
        c.r,
        c.g,
        c.b,
        f64::from(c.a) / 255.0
    )
}
