//! A protocol client verifies first-configure ordering and advisory-only bounds.
use super::*;
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::xdg::{
    decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Debug, PartialEq)]
enum Event {
    Bounds(i32, i32),
    Size(i32, i32),
    Configure,
}

#[derive(Default)]
struct ClientState {
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    decoration: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    events: Vec<Event>,
}

fn initial_events(compositor: &HeadlessCompositor, server_chrome: bool) -> Vec<Event> {
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    let _registry = connection.display().get_registry(&handle, ());
    let mut state = ClientState::default();
    queue.roundtrip(&mut state).unwrap();
    let surface = state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&handle, ());
    let xdg_surface = state
        .wm_base
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &handle, ());
    let toplevel = xdg_surface.get_toplevel(&handle, ());
    toplevel.set_app_id("dev.hearthspace.BoundsTest".into());
    // A legitimate client minimum may exceed the output. Bounds are a hint:
    // the compositor must still leave the initial requested size at (0, 0).
    toplevel.set_min_size(2000, 1500);
    let _decoration = server_chrome.then(|| {
        let decoration =
            state
                .decoration
                .as_ref()
                .unwrap()
                .get_toplevel_decoration(&toplevel, &handle, ());
        decoration.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        decoration
    });
    queue.roundtrip(&mut state).unwrap();
    assert!(
        state.events.is_empty(),
        "configure arrived before initial commit: {:?}",
        state.events
    );
    surface.commit();
    queue.roundtrip(&mut state).unwrap();
    state.events
}

#[test]
fn initial_window_bounds_precede_configure_and_leave_client_size_advisory() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    assert_eq!(
        initial_events(&compositor, false),
        vec![
            Event::Bounds(1280, 746),
            Event::Size(0, 0),
            Event::Configure
        ]
    );
    assert_eq!(
        initial_events(&compositor, true),
        vec![
            Event::Bounds(1280, 702),
            Event::Size(0, 0),
            Event::Configure
        ]
    );
    assert_eq!(compositor.send_text_command("zoom-out"), "ok\n");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut stream = UnixStream::connect(&compositor.command_socket_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(b"status\n").unwrap();
        let header = read_line(&mut stream);
        let len: usize = header.strip_prefix("ok ").unwrap().trim().parse().unwrap();
        let mut bytes = vec![0; len];
        stream.read_exact(&mut bytes).unwrap();
        let state: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if state["viewport"]["scale"] == 0.8 {
            break;
        }
        assert!(Instant::now() < deadline, "zoom did not settle");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        initial_events(&compositor, true),
        vec![
            // Floor the 746px work area at 0.8 zoom before subtracting
            // the 44px server decoration: floor(746 / 0.8) - 44.
            Event::Bounds(1600, 888),
            Event::Size(0, 0),
            Event::Configure
        ]
    );
}

impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), handle, ()))
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind(name, version.min(6), handle, ()))
                }
                "zxdg_decoration_manager_v1" => {
                    state.decoration = Some(registry.bind(name, 1, handle, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ClientState {
    fn event(
        _: &mut Self,
        object: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            object.pong(serial);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::ConfigureBounds { width, height } => {
                state.events.push(Event::Bounds(width, height))
            }
            xdg_toplevel::Event::Configure { width, height, .. } => {
                state.events.push(Event::Size(width, height))
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for ClientState {
    fn event(
        state: &mut Self,
        object: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            object.ack_configure(serial);
            state.events.push(Event::Configure);
        }
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(ClientState: ignore zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1);
