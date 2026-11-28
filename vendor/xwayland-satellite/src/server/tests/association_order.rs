//! X11 and Wayland use independent streams; neither serial arrival order is guaranteed.
use super::*;

fn window(f: &mut TestFixture<FakeXConnection>, id: Window) {
    f.new_window(
        id,
        false,
        WindowData {
            mapped: false,
            fullscreen: false,
            dims: WindowDims {
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            },
        },
    );
}
fn surface(comp: &Compositor, serial: u32) -> (TestObject<WlBuffer>, TestObject<WlSurface>) {
    let (buffer, _scratch) = comp.create_surface();
    let surface = TestObject::<WlSurface>::from_request(
        &comp.compositor.obj,
        Req::<WlCompositor>::CreateSurface {},
    );
    let xwl = TestObject::<XwaylandSurfaceV1>::from_request(
        &comp.shell.obj,
        Req::<XwaylandShellV1>::GetXwaylandSurface {
            surface: surface.obj.clone(),
        },
    );
    xwl.send_request(Req::<XwaylandSurfaceV1>::SetSerial {
        serial_lo: serial,
        serial_hi: 0,
    })
    .unwrap();
    surface
        .send_request(Req::<WlSurface>::Attach {
            buffer: Some(buffer.obj.clone()),
            x: 0,
            y: 0,
        })
        .unwrap();
    surface.send_request(Req::<WlSurface>::Commit {}).unwrap();
    (buffer, surface)
}
fn assert_mapped_buffer(f: &mut TestFixture<FakeXConnection>, id: testwl::SurfaceId) {
    assert!(
        matches!(
            f.testwl.get_surface_data(id).unwrap().role,
            Some(SurfaceRole::Toplevel(_))
        ),
        "paired and mapped window must receive its XDG role"
    );
    f.testwl.configure_toplevel(id, 50, 50, Vec::new());
    f.run();
    assert!(
        f.testwl.get_surface_data(id).unwrap().buffer.is_some(),
        "initial buffer must survive late association and configure"
    );
}
#[test]
fn wayland_serial_before_x11_serial_maps_and_preserves_initial_buffer() {
    let (mut f, comp) = TestFixture::new_with_compositor();
    let id = Window::new(400);
    window(&mut f, id);
    f.satellite.map_window(id);
    let (_buffer, _surface) = surface(&comp, 400);
    f.run();
    let host = f.check_new_surface();
    assert!(f.testwl.get_surface_data(host).unwrap().role.is_none());
    f.satellite.set_window_serial(id, [400, 0]);
    f.run();
    assert_mapped_buffer(&mut f, host);
}
#[test]
fn association_before_map_waits_then_maps_with_initial_buffer() {
    let (mut f, comp) = TestFixture::new_with_compositor();
    let id = Window::new(401);
    window(&mut f, id);
    f.satellite.set_window_serial(id, [401, 0]);
    let (_buffer, _surface) = surface(&comp, 401);
    f.run();
    let host = f.check_new_surface();
    assert!(f.testwl.get_surface_data(host).unwrap().role.is_none());
    f.satellite.map_window(id);
    f.run();
    assert_mapped_buffer(&mut f, host);
}
#[test]
fn destroyed_pending_window_cannot_bind_its_surface_to_a_reused_xid() {
    let (mut f, comp) = TestFixture::new_with_compositor();
    let id = Window::new(402);
    window(&mut f, id);
    f.satellite.map_window(id);
    let (_buffer, _old) = surface(&comp, 402);
    f.run();
    let old_host = f.check_new_surface();
    f.satellite.destroy_window(id);
    window(&mut f, id);
    f.satellite.map_window(id);
    f.satellite.set_window_serial(id, [403, 0]);
    let (_buffer, _new) = surface(&comp, 403);
    f.run();
    let new_host = f.check_new_surface();
    assert_ne!(old_host, new_host);
    assert!(f.testwl.get_surface_data(old_host).unwrap().role.is_none());
    assert_mapped_buffer(&mut f, new_host);
}
