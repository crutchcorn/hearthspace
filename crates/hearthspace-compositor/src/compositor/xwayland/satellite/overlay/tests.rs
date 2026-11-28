//! Real private-protocol requests with App output routing; no renderer or seat takeover.
use super::*;
use crate::compositor::{
    CalloopData, OutputDescriptor, client::ClientState, frame_callbacks::test_support::app::new_app,
};
use smithay::{
    output::{PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
};
use std::{
    collections::{BTreeSet, HashMap},
    os::unix::net::UnixStream,
    sync::Arc,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{wl_compositor, wl_output, wl_registry, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_server::Display;

mod wire {
    #![allow(
        dead_code,
        non_camel_case_types,
        unused_imports,
        non_upper_case_globals,
        non_snake_case,
        unused_unsafe,
        clippy::all
    )]
    use wayland_client::{self, protocol::*};
    use wayland_protocols::xdg::shell::client::*;
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::xdg::shell::client::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../vendor/xwayland-satellite/protocols/xwayland-satellite-overlay-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!(
        "../../vendor/xwayland-satellite/protocols/xwayland-satellite-overlay-v1.xml"
    );
}
use wire::xwayland_satellite_overlay_v1::XwaylandSatelliteOverlayV1 as Overlay;
#[derive(Default)]
struct Client {
    compositor: Option<wl_compositor::WlCompositor>,
    xdg: Option<xdg_wm_base::XdgWmBase>,
    overlay: Option<Overlay>,
    output_names: HashMap<u32, String>,
    entered: BTreeSet<u32>,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::GlobalRemove { name } = event {
            state.entered.remove(&name);
            state.output_names.remove(&name);
            return;
        }
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_output" => {
                let _: wl_output::WlOutput = registry.bind(name, version.min(4), q, name);
            }
            "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(6), q, ())),
            "xdg_wm_base" => state.xdg = Some(registry.bind(name, version.min(6), q, ())),
            "xwayland_satellite_overlay_v1" => state.overlay = Some(registry.bind(name, 1, q, ())),
            _ => {}
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(
        _: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            proxy.ack_configure(serial);
        }
    }
}
delegate_noop!(Client: wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wayland_client::protocol::wl_callback::WlCallback);
impl Dispatch<wl_output::WlOutput, u32> for Client {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.output_names.insert(*global, name);
        }
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_surface::Event::Enter { output } => {
                state.entered.insert(*output.data::<u32>().unwrap());
            }
            wl_surface::Event::Leave { output } => {
                state.entered.remove(output.data::<u32>().unwrap());
            }
            _ => {}
        }
    }
}
delegate_noop!(Client: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(Client: Overlay);

fn sync(
    connection: &Connection,
    queue: &mut EventQueue<Client>,
    client: &mut Client,
    display: &mut Display<App>,
    app: &mut App,
) {
    connection.display().sync(&queue.handle(), ());
    connection.flush().unwrap();
    display.dispatch_clients(app).unwrap();
    display.flush_clients().unwrap();
    if let Some(read) = queue.prepare_read() {
        read.read().unwrap();
    }
    queue.dispatch_pending(client).unwrap();
}
fn descriptors() -> Vec<OutputDescriptor> {
    [("A", (800, 600), 1), ("B", (1600, 1200), 2)]
        .map(|(name, size, scale)| OutputDescriptor {
            name: name.into(),
            size: size.into(),
            scale,
            refresh: 60_000,
            properties: PhysicalProperties {
                size: (300, 200).into(),
                subpixel: Subpixel::Unknown,
                make: "Overlay fixture".into(),
                model: name.into(),
                serial_number: name.into(),
            },
        })
        .into()
}
#[test]
fn overlay_protocol_routes_to_secondary_output_and_falls_back_after_removal() {
    let mut display = Display::new().unwrap();
    let mut dh = display.handle();
    let events = EventLoop::<CalloopData>::try_new().unwrap();
    let mut app = new_app(&dh, events.handle());
    app.sync_connector_outputs(&dh, descriptors());
    let (server, peer) = UnixStream::pair().unwrap();
    let authorized = dh
        .insert_client(server, Arc::new(ClientState::satellite()))
        .unwrap();
    app.satellite_interaction
        .advertise(&dh, authorized.id(), &app.primary_selection_state);
    let connection = Connection::from_socket(peer).unwrap();
    let mut queue = connection.new_event_queue();
    let mut client = Client::default();
    connection.display().get_registry(&queue.handle(), ());
    sync(&connection, &mut queue, &mut client, &mut display, &mut app);
    sync(&connection, &mut queue, &mut client, &mut display, &mut app);
    let surface = client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&queue.handle(), ());
    let xdg = client
        .xdg
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &queue.handle(), ());
    let top = xdg.get_toplevel(&queue.handle(), ());
    client
        .overlay
        .as_ref()
        .unwrap()
        .set_overlay(&top, "B".into(), 3);
    surface.commit();
    sync(&connection, &mut queue, &mut client, &mut display, &mut app);
    assert_eq!(app.windows.len(), 1);
    assert_eq!(app.windows[0].kind, ManagedWindowKind::X11Overlay);
    // Supply mapping and content extent; placement/visibility/projection remain production code.
    app.windows[0].mapped = true;
    app.windows[0].content_bbox_size = (200, 80).into();
    let b_global = *client
        .output_names
        .iter()
        .find(|(_, name)| name.as_str() == "B")
        .unwrap()
        .0;
    assert_eq!(client.entered, BTreeSet::from([b_global]));
    let b = app.output_logical_rect_for_name(Some("B")).unwrap();
    let rect = app.window_projection(0).bounds;
    assert_eq!(
        rect.loc,
        (b.loc.x + b.size.w - 224, b.loc.y + b.size.h - 104).into()
    );
    assert_eq!(app.window_projection(0).scale, 1.);
    assert!(app.window_visible_on_output(0, b));
    assert!(!app.window_visible_on_output(0, app.primary_output_logical_rect()));
    app.viewport_scale = 0.5;
    app.viewport_offset = crate::geometry::CanvasPoint { x: 1000, y: -200 };
    assert_eq!(app.window_projection(0).bounds, rect);
    app.sync_connector_outputs(&dh, descriptors().into_iter().take(1).collect());
    app.refresh_normal_window_outputs();
    sync(&connection, &mut queue, &mut client, &mut display, &mut app);
    let a_global = *client
        .output_names
        .iter()
        .find(|(_, name)| name.as_str() == "A")
        .unwrap()
        .0;
    assert_eq!(
        client.entered,
        BTreeSet::from([a_global]),
        "fallback placement must also reassign the live surface output"
    );
    let primary = app.primary_output_logical_rect();
    assert!(primary.contains_rect(app.window_projection(0).bounds));

    let (server, peer) = UnixStream::pair().unwrap();
    dh.insert_client(server, Arc::new(ClientState::default()))
        .unwrap();
    let ordinary = Connection::from_socket(peer).unwrap();
    let mut queue = ordinary.new_event_queue();
    let mut ordinary_client = Client::default();
    ordinary.display().get_registry(&queue.handle(), ());
    sync(
        &ordinary,
        &mut queue,
        &mut ordinary_client,
        &mut display,
        &mut app,
    );
    sync(
        &ordinary,
        &mut queue,
        &mut ordinary_client,
        &mut display,
        &mut app,
    );
    assert!(
        ordinary_client.overlay.is_none(),
        "ordinary applications cannot claim overlay roles"
    );
}
