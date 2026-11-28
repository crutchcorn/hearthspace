use super::*;

pub(super) fn overlay(
    c: &HeadlessCompositor,
    display: &str,
    utility: bool,
    complete: bool,
) -> (RustConnection, Window) {
    overlay_at(c, display, utility, complete, (900, 600))
}

pub(super) fn overlay_at(
    c: &HeadlessCompositor,
    display: &str,
    utility: bool,
    complete: bool,
    position: (i16, i16),
) -> (RustConnection, Window) {
    let options = Options {
        utility,
        complete,
        position,
        unmanaged: utility,
        ..Default::default()
    };
    with_options(c, display, options)
}

pub(super) struct Options {
    pub utility: bool,
    pub complete: bool,
    pub position: (i16, i16),
    pub unmanaged: bool,
    pub parent: Option<Window>,
    pub prefer_normal: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            utility: true,
            complete: true,
            position: (900, 600),
            unmanaged: true,
            parent: None,
            prefer_normal: false,
        }
    }
}

pub(super) fn with_options(
    c: &HeadlessCompositor,
    display: &str,
    options: Options,
) -> (RustConnection, Window) {
    let Options {
        utility,
        complete,
        position,
        unmanaged,
        parent,
        prefer_normal,
    } = options;
    let (connection, screen) = x11rb::connect(Some(display)).unwrap();
    let screen = &connection.setup().roots[screen];
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            window,
            screen.root,
            position.0,
            position.1,
            200,
            80,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .override_redirect(u32::from(unmanaged))
                .background_pixel(0x20a060)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"generic-overlay\0GenericOverlayFixture\0",
        )
        .unwrap();
    let atom = |name: &[u8]| {
        connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let kind = atom(if utility {
        b"_NET_WM_WINDOW_TYPE_UTILITY"
    } else {
        b"_NET_WM_WINDOW_TYPE_NOTIFICATION"
    });
    if let Some(parent) = parent {
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                &[parent],
            )
            .unwrap();
    }
    let types = if prefer_normal {
        vec![atom(b"_NET_WM_WINDOW_TYPE_NORMAL"), kind]
    } else {
        vec![kind]
    };
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            atom(b"_NET_WM_WINDOW_TYPE"),
            AtomEnum::ATOM,
            &types,
        )
        .unwrap();
    let mut states = vec![
        atom(b"_NET_WM_STATE_ABOVE"),
        atom(b"_NET_WM_STATE_SKIP_TASKBAR"),
    ];
    if complete {
        states.push(atom(b"_NET_WM_STATE_SKIP_PAGER"));
    }
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            atom(b"_NET_WM_STATE"),
            AtomEnum::ATOM,
            &states,
        )
        .unwrap();
    connection.map_window(window).unwrap();
    connection.flush().unwrap();
    // Synchronize the server independently of compositor presentation.
    connection.get_input_focus().unwrap().reply().unwrap();
    let _ = c;
    (connection, window)
}

pub(super) fn diagnostics(status: &Value) -> Value {
    let Some(display) = status["xwayland_display"].as_str() else {
        return Value::Null;
    };
    let Ok((connection, screen)) = x11rb::connect(Some(display)) else {
        return Value::Null;
    };
    let root = connection.setup().roots[screen].root;
    let geometry = connection.get_geometry(root).unwrap().reply().unwrap();
    let tree = connection.query_tree(root).unwrap().reply().unwrap();
    let mut rows = Vec::new();
    for window in tree.children {
        let attributes = connection.get_window_attributes(window).unwrap().reply();
        let geometry = connection.get_geometry(window).unwrap().reply();
        let mut properties = serde_json::Map::new();
        for name in [
            "WL_SURFACE_SERIAL",
            "WL_SURFACE_ID",
            "_NET_WM_WINDOW_TYPE",
            "_NET_WM_STATE",
        ] {
            let atom = connection
                .intern_atom(true, name.as_bytes())
                .unwrap()
                .reply()
                .unwrap()
                .atom;
            if atom == 0 {
                continue;
            }
            if let Ok(property) = connection
                .get_property(false, window, atom, AtomEnum::ANY, 0, 64)
                .unwrap()
                .reply()
            {
                properties.insert(name.into(),serde_json::json!({"type":property.type_,"format":property.format,"values":property.value32().map(|v|v.collect::<Vec<_>>())}));
            }
        }
        rows.push(serde_json::json!({"window":window,"attributes":format!("{attributes:?}"),"geometry":format!("{geometry:?}"),"properties":properties}));
    }
    serde_json::json!({"root_geometry":format!("{geometry:?}"),"windows":rows})
}
