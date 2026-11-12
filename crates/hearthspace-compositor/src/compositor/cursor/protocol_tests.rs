//! Real cursor-shape and wl_pointer requests against App over private sockets.
mod fixture;

use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

use super::{CursorIcon, cursor_image_for_icon};
use fixture::{Fixture, Peer};

#[test]
fn all_protocol_shapes_preserve_intent_and_have_packaged_artwork() {
    let mut f = Fixture::new();
    let surface = f.client.surface();
    f.sync();
    let serial = f.focus(Some(&surface));
    assert_eq!(f.client.state.enter, Some(serial));
    let device = f.shape();
    let arrow = cursor_image_for_icon(CursorIcon::Default);
    for (shape, icon) in [
        (Shape::Default, CursorIcon::Default),
        (Shape::ContextMenu, CursorIcon::ContextMenu),
        (Shape::Help, CursorIcon::Help),
        (Shape::Pointer, CursorIcon::Pointer),
        (Shape::Progress, CursorIcon::Progress),
        (Shape::Wait, CursorIcon::Wait),
        (Shape::Cell, CursorIcon::Cell),
        (Shape::Crosshair, CursorIcon::Crosshair),
        (Shape::Text, CursorIcon::Text),
        (Shape::VerticalText, CursorIcon::VerticalText),
        (Shape::Alias, CursorIcon::Alias),
        (Shape::Copy, CursorIcon::Copy),
        (Shape::Move, CursorIcon::Move),
        (Shape::NoDrop, CursorIcon::NoDrop),
        (Shape::NotAllowed, CursorIcon::NotAllowed),
        (Shape::Grab, CursorIcon::Grab),
        (Shape::Grabbing, CursorIcon::Grabbing),
        (Shape::EResize, CursorIcon::EResize),
        (Shape::NResize, CursorIcon::NResize),
        (Shape::NeResize, CursorIcon::NeResize),
        (Shape::NwResize, CursorIcon::NwResize),
        (Shape::SResize, CursorIcon::SResize),
        (Shape::SeResize, CursorIcon::SeResize),
        (Shape::SwResize, CursorIcon::SwResize),
        (Shape::WResize, CursorIcon::WResize),
        (Shape::EwResize, CursorIcon::EwResize),
        (Shape::NsResize, CursorIcon::NsResize),
        (Shape::NeswResize, CursorIcon::NeswResize),
        (Shape::NwseResize, CursorIcon::NwseResize),
        (Shape::ColResize, CursorIcon::ColResize),
        (Shape::RowResize, CursorIcon::RowResize),
        (Shape::AllScroll, CursorIcon::AllScroll),
        (Shape::ZoomIn, CursorIcon::ZoomIn),
        (Shape::ZoomOut, CursorIcon::ZoomOut),
        (Shape::DndAsk, CursorIcon::DndAsk),
        (Shape::AllResize, CursorIcon::AllResize),
    ] {
        device.set_shape(serial, shape);
        f.sync();
        assert_eq!(f.app.cursor_icon, icon);
        assert!(f.app.cursor_visible);
        assert!(f.app.cursor_surface.is_none());
        let image = cursor_image_for_icon(icon);
        assert_eq!((image.width, image.height), (24, 24), "{icon:?}");
        assert_eq!(image.pixels_rgba.len(), 24 * 24 * 4);
        assert!((0..24).contains(&image.hotspot.0));
        assert!((0..24).contains(&image.hotspot.1));
        if icon != CursorIcon::Default {
            assert_ne!(
                image.pixels_rgba, arrow.pixels_rgba,
                "{icon:?} fell back to arrow"
            );
        }
    }
}

#[test]
fn shapes_require_current_focus_and_latest_enter_serial() {
    let mut f = Fixture::new();
    let surface = f.client.surface();
    f.sync();
    let device = f.shape();
    device.set_shape(0, Shape::Text);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Default);
    let old = f.focus(Some(&surface));
    device.set_shape(old, Shape::Text);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Text);
    f.focus(None);
    let current = f.focus(Some(&surface));
    device.set_shape(current, Shape::Pointer);
    f.sync();
    device.set_shape(old, Shape::Wait);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Pointer);

    let mut other = Peer::new(&f.display);
    other.sync(&mut f.display, &mut f.app);
    other.sync(&mut f.display, &mut f.app);
    let other_device = other.state.manager.as_ref().unwrap().get_pointer(
        other.state.pointer.as_ref().unwrap(),
        &other.queue.handle(),
        (),
    );
    // Knowing the serial is insufficient: another client does not own focus.
    other_device.set_shape(current, Shape::Wait);
    other.sync(&mut f.display, &mut f.app);
    assert_eq!(f.app.cursor_icon, CursorIcon::Pointer);
    let other_surface = other.surface();
    other.sync(&mut f.display, &mut f.app);
    let other_server = other.server_surface(&f.display, &other_surface);
    let serial = f.focus_server(Some(other_server));
    device.set_shape(serial, Shape::Wait);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Default);
}

#[test]
fn clients_can_mix_surfaces_hidden_and_shapes_without_scene_redraw_on_native() {
    let mut f = Fixture::new();
    let surface = f.client.surface();
    let cursor = f.client.surface();
    f.sync();
    let serial = f.focus(Some(&surface));
    let device = f.shape();
    let pointer = f.client.state.pointer.as_ref().unwrap().clone();
    pointer.set_cursor(serial, Some(&cursor), 2, 3);
    f.sync();
    assert!(f.app.cursor_surface.is_some());
    f.app.independent_cursor = true;
    f.app.needs_redraw = false;
    device.set_shape(serial, Shape::Text);
    f.sync();
    assert!(f.app.cursor_surface.is_none());
    assert!(f.app.cursor_visible);
    assert!(!f.app.needs_redraw);
    pointer.set_cursor(serial, None, 0, 0);
    f.sync();
    assert!(!f.app.cursor_visible);
    f.app.independent_cursor = false;
    device.set_shape(serial, Shape::Pointer);
    f.sync();
    assert!(f.app.cursor_visible);
    assert!(f.app.needs_redraw);
    device.destroy();
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Pointer);
    pointer.set_cursor(serial, Some(&cursor), 1, 1);
    f.sync();
    assert!(f.app.cursor_surface.is_some());
}

#[test]
fn removing_pointer_capability_makes_existing_shape_device_inert() {
    let mut f = Fixture::new();
    let surface = f.client.surface();
    f.sync();
    let serial = f.focus(Some(&surface));
    let device = f.shape();
    device.set_shape(serial, Shape::Text);
    f.sync();
    f.app._seat.remove_pointer();
    f.sync();
    device.set_shape(serial, Shape::Wait);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Text);
}

#[test]
fn named_shape_survives_motion_and_output_reconciliation_inside_client() {
    use smithay::backend::input::InputTime;
    let mut f = Fixture::new();
    f.client.map_window(&mut f.display, &mut f.app);
    f.app
        .apply_pointer_motion((120., 120.).into(), InputTime::now());
    f.sync();
    let serial = f
        .client
        .state
        .enter
        .expect("pointer entered client content");
    let device = f.shape();
    device.set_shape(serial, Shape::Text);
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Text);
    f.app
        .apply_pointer_motion((130., 130.).into(), InputTime::now());
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Text);
    f.app.reconcile_pointer_after_output_geometry_change();
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Text);
    f.app
        .apply_pointer_motion((700., 500.).into(), InputTime::now());
    f.sync();
    assert_eq!(f.app.cursor_icon, CursorIcon::Default);
}

#[test]
fn invalid_shape_reports_protocol_error() {
    use wayland_client::{Proxy, WEnum};
    use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
        Error, Request,
    };
    let mut f = Fixture::new();
    let device = f.shape();
    device
        .send_request(Request::SetShape {
            serial: 0,
            shape: WEnum::Unknown(u32::MAX),
        })
        .unwrap();
    // An invalid enum is a protocol error even without pointer focus.
    // Send a sync too, so the unpatched implementation returns promptly and
    // fails the error assertion rather than blocking for an event forever.
    f.client
        .connection
        .display()
        .sync(&f.client.queue.handle(), ());
    f.client.connection.flush().unwrap();
    f.display.dispatch_clients(&mut f.app).unwrap();
    f.display.flush_clients().unwrap();
    if let Some(read) = f.client.queue.prepare_read() {
        let _ = read.read();
    }
    let _ = f.client.queue.dispatch_pending(&mut f.client.state);
    let error = f
        .client
        .connection
        .protocol_error()
        .expect("invalid_shape protocol error");
    assert_eq!(error.code, u32::from(Error::InvalidShape));
    assert_eq!(error.object_interface, "wp_cursor_shape_device_v1");
}
