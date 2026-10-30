use super::*;
use wayland_client::{
    Dispatch, QueueHandle, delegate_noop, event_created_child,
    protocol::{wl_registry, wl_shm_pool},
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1 as toplevel, ext_foreign_toplevel_list_v1 as list,
    },
    image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    image_copy_capture::v1::client::ext_image_copy_capture_session_v1 as session,
};

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        state.globals.push(interface.clone());
        match interface.as_str() {
            "wl_shm" => state.shm = Some(registry.bind(name, 1, q, ())),
            "wl_output" => state.output = Some(registry.bind(name, version.min(4), q, ())),
            "ext_foreign_toplevel_list_v1" => state.list = Some(registry.bind(name, 1, q, ())),
            "ext_foreign_toplevel_image_capture_source_manager_v1" => {
                state.source = Some(registry.bind(name, 1, q, ()))
            }
            "ext_output_image_capture_source_manager_v1" => {
                state.output_source = Some(registry.bind(name, 1, q, ()))
            }
            "ext_image_copy_capture_manager_v1" => {
                state.manager = Some(registry.bind(name, 1, q, ()))
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    event_created_child!(State, ExtForeignToplevelListV1, [0 => (ExtForeignToplevelHandleV1, ())]);
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: list::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let list::Event::Toplevel { toplevel } = event {
            state.toplevels.push(Toplevel {
                object: toplevel,
                identifier: String::new(),
                title: String::new(),
                app_id: String::new(),
                closed: false,
            });
        }
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        object: &ExtForeignToplevelHandleV1,
        event: toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(handle) = state.toplevels.iter_mut().find(|t| &t.object == object) else {
            return;
        };
        match event {
            toplevel::Event::Identifier { identifier } => handle.identifier = identifier,
            toplevel::Event::Title { title } => handle.title = title,
            toplevel::Event::AppId { app_id } => handle.app_id = app_id,
            toplevel::Event::Closed => handle.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: session::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let s = state.sessions.get_mut(id).unwrap();
        match event {
            session::Event::BufferSize { width, height } => {
                s.size = Some((width as i32, height as i32))
            }
            session::Event::Stopped => s.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: FrameEvent,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.frames.get_mut(id).unwrap().push(event);
    }
}

delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore ExtImageCaptureSourceV1);
delegate_noop!(State: ignore ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtForeignToplevelImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtImageCopyCaptureManagerV1);
