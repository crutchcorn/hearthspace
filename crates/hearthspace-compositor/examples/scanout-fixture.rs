//! Opaque fullscreen DMA-BUF fixture with a hidden client cursor.
//! Run only inside an owned native session. See docs/DRM.md.
use smithay::{
    backend::{
        allocator::{
            Allocator, Buffer, Fourcc, Modifier,
            dmabuf::AsDmabuf,
            gbm::{GbmAllocator, GbmBuffer, GbmBufferFlags, GbmDevice},
        },
        egl::{EGLContext, EGLDisplay},
        renderer::{Bind, Color32F, Frame, Renderer, gles::GlesRenderer},
    },
    utils::{Rectangle, Transform},
};
use std::{fs::OpenOptions, os::fd::AsFd, sync::Arc};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_region, wl_registry, wl_seat,
        wl_surface,
    },
};
use wayland_protocols::{
    wp::linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Default)]
struct Client {
    compositor: Option<wl_compositor::WlCompositor>,
    dmabuf: Option<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    size: (i32, i32),
    configured: bool,
    frame: bool,
    closed: bool,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        s: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => s.compositor = Some(registry.bind(name, version.min(4), q, ())),
                "zwp_linux_dmabuf_v1" => {
                    s.dmabuf = Some(registry.bind(name, version.min(3), q, ()))
                }
                "xdg_wm_base" => s.shell = Some(registry.bind(name, version.min(3), q, ())),
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), q, ());
                    seat.get_pointer(q, ());
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        shell: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            shell.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(
        s: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            s.configured = true;
            s.frame = true;
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
    fn event(
        s: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } if width > 0 && height > 0 => {
                s.size = (width, height)
            }
            xdg_toplevel::Event::Close => s.closed = true,
            _ => {}
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for Client {
    fn event(
        _: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } = event {
            pointer.set_cursor(serial, None, 0, 0);
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for Client {
    fn event(
        s: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        s.frame = true;
    }
}
impl Dispatch<wl_buffer::WlBuffer, usize> for Client {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        generation: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            println!("released generation {generation}");
        }
    }
}
delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_region::WlRegion);
delegate_noop!(Client: ignore wl_seat::WlSeat);
delegate_noop!(Client: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);
delegate_noop!(Client: ignore zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node = std::env::args().nth(1).ok_or("render node path required")?;
    let gbm = GbmDevice::new(Arc::new(
        OpenOptions::new().read(true).write(true).open(node)?,
    ))?;
    // SAFETY: the GBM display and context remain alive until the renderer drops.
    let display = unsafe { EGLDisplay::new(gbm.clone())? };
    let context = EGLContext::new(&display)?;
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    // LINEAR is a layout guarantee even on GBM drivers reporting an implicit
    // modifier for the allocation. Never relabel an unconstrained allocation.
    let mut allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::LINEAR);
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let q = queue.handle();
    let mut client = Client::default();
    connection.display().get_registry(&q, ());
    queue.roundtrip(&mut client)?;
    let compositor = client.compositor.clone().ok_or("wl_compositor missing")?;
    let surface = compositor.create_surface(&q, ());
    let xdg = client
        .shell
        .as_ref()
        .ok_or("xdg_wm_base missing")?
        .get_xdg_surface(&surface, &q, ());
    let toplevel = xdg.get_toplevel(&q, ());
    toplevel.set_app_id("dev.hearthspace.ScanoutFixture".into());
    toplevel.set_title("Fullscreen DMA-BUF scanout fixture".into());
    toplevel.set_fullscreen(None);
    surface.commit();
    // Each generation remains immutable. Retain old buffers through resizes;
    // the bounded native harness owns the fixture's lifetime and memory.
    let mut buffers: Vec<(GbmBuffer, wl_buffer::WlBuffer)> = Vec::new();
    let mut size = (0, 0);
    while !client.closed {
        queue.blocking_dispatch(&mut client)?;
        if !client.configured || !client.frame {
            continue;
        }
        if client.size != size {
            size = client.size;
            if size.0 <= 0 || size.1 <= 0 || i64::from(size.0) * i64::from(size.1) > 16_777_216 {
                return Err("invalid fixture size".into());
            }
            let buffer = allocator.create_buffer(
                size.0 as u32,
                size.1 as u32,
                Fourcc::Xrgb8888,
                &[Modifier::Linear],
            )?;
            let mut dmabuf = buffer.export()?;
            assert!(matches!(
                dmabuf.format().modifier,
                Modifier::Linear | Modifier::Invalid
            ));
            {
                let mut target = renderer.bind(&mut dmabuf)?;
                let mut frame = renderer.render(&mut target, size.into(), Transform::Normal)?;
                frame.clear(
                    Color32F::new(0., 0.8, 0.4, 1.),
                    &[Rectangle::from_size(size.into())],
                )?;
                frame.clear(
                    Color32F::new(1., 0., 0., 1.),
                    &[Rectangle::from_size((size.0 / 4, size.1 / 4).into())],
                )?;
                frame.finish()?.wait()?;
            }
            let params = client
                .dmabuf
                .as_ref()
                .ok_or("linux-dmabuf missing")?
                .create_params(&q, ());
            for (plane, ((fd, stride), offset)) in dmabuf
                .handles()
                .zip(dmabuf.strides())
                .zip(dmabuf.offsets())
                .enumerate()
            {
                params.add(fd.as_fd(), plane as u32, offset, stride, 0, 0);
            }
            let wl_buffer = params.create_immed(
                size.0,
                size.1,
                Fourcc::Xrgb8888 as u32,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &q,
                buffers.len(),
            );
            params.destroy();
            buffers.push((buffer, wl_buffer));
            let opaque = compositor.create_region(&q, ());
            opaque.add(0, 0, size.0, size.1);
            surface.set_opaque_region(Some(&opaque));
            opaque.destroy();
            xdg.set_window_geometry(0, 0, size.0, size.1);
            println!(
                "generation {} size {}x{} allocation {:?} protocol modifier Linear",
                buffers.len() - 1,
                size.0,
                size.1,
                dmabuf.format()
            );
        }
        surface.attach(Some(&buffers.last().unwrap().1), 0, 0);
        surface.damage_buffer(0, 0, size.0, size.1);
        surface.frame(&q, ());
        surface.commit();
        client.frame = false;
        connection.flush()?;
    }
    Ok(())
}
