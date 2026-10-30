use std::{collections::HashMap, fs::File, os::unix::fs::FileExt};

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_registry, wl_subcompositor, wl_subsurface,
        wl_surface,
    },
};
use wayland_protocols::{
    wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1, zwp_linux_dmabuf_v1,
    },
    xdg::shell::client::{xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Clone, Debug, Default)]
pub(super) struct Feedback {
    pub(super) device: Option<u64>,
    pub(super) table: Vec<Format>,
    pub(super) targets: Vec<u64>,
    pub(super) formats: Vec<Format>,
    pub(super) flags: Vec<u32>,
    pub(super) tranches: usize,
}

#[derive(Default)]
pub(super) struct FeedbackEvents {
    pending: Feedback,
    pub(super) complete: Vec<Feedback>,
}

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) compositor: Option<wl_compositor::WlCompositor>,
    pub(super) subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    pub(super) xdg: Option<xdg_wm_base::XdgWmBase>,
    pub(super) globals: Vec<(u32, u32, zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1)>,
    pub(super) removed: Vec<u32>,
    pub(super) feedback: HashMap<usize, FeedbackEvents>,
    pub(super) failed: Vec<usize>,
    pub(super) legacy: HashMap<usize, Vec<Format>>,
    pub(super) synced: bool,
    pub(super) bind_version: u32,
}

impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), queue, ()))
                }
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, queue, ())),
                "xdg_wm_base" => state.xdg = Some(registry.bind(name, version.min(6), queue, ())),
                "zwp_linux_dmabuf_v1" => {
                    let id = state.globals.len();
                    state.globals.push((
                        name,
                        version,
                        registry.bind(name, version.min(state.bind_version), queue, id),
                    ));
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => state.removed.push(name),
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, usize> for ClientState {
    fn event(
        state: &mut Self,
        _: &zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
        event: zwp_linux_dmabuf_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_linux_dmabuf_v1::Event::Modifier {
            format,
            modifier_hi,
            modifier_lo,
        } = event
        {
            state.legacy.entry(*id).or_default().push(Format {
                code: Fourcc::try_from(format).unwrap(),
                modifier: Modifier::from((u64::from(modifier_hi) << 32) | u64::from(modifier_lo)),
            });
        }
    }
}

impl Dispatch<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1, usize> for ClientState {
    fn event(
        state: &mut Self,
        _: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_linux_dmabuf_feedback_v1::Event;
        let events = state.feedback.entry(*id).or_default();
        match event {
            Event::MainDevice { device } => {
                events.pending.device = Some(u64::from_ne_bytes(device.try_into().unwrap()))
            }
            Event::FormatTable { fd, size } => {
                let mut bytes = vec![0; size as usize];
                // SCM_RIGHTS duplicates share the table's file offset. Read
                // positionally so another feedback object sees the full table.
                File::from(fd).read_exact_at(&mut bytes, 0).unwrap();
                assert_eq!(bytes.len() % 16, 0);
                events.pending.table = bytes
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .map(|entry| Format {
                        code: Fourcc::try_from(u32::from_ne_bytes(entry[..4].try_into().unwrap()))
                            .unwrap(),
                        modifier: Modifier::from(u64::from_ne_bytes(
                            entry[8..].try_into().unwrap(),
                        )),
                    })
                    .collect();
            }
            Event::TrancheTargetDevice { device } => events
                .pending
                .targets
                .push(u64::from_ne_bytes(device.try_into().unwrap())),
            Event::TrancheFormats { indices } => {
                assert_eq!(indices.len() % 2, 0);
                for index in indices.as_chunks::<2>().0 {
                    events
                        .pending
                        .formats
                        .push(events.pending.table[usize::from(u16::from_ne_bytes(*index))]);
                }
            }
            Event::TrancheFlags { flags } => events.pending.flags.push(flags.into()),
            Event::TrancheDone => events.pending.tranches += 1,
            Event::Done => events.complete.push(std::mem::take(&mut events.pending)),
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, usize> for ClientState {
    fn event(
        state: &mut Self,
        _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Failed => state.failed.push(*id),
            zwp_linux_buffer_params_v1::Event::Created { .. } => {
                panic!("test import handler must reject the non-GPU backing file")
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(ClientState, zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, [
        0 => (wl_buffer::WlBuffer, ())
    ]);
}

impl Dispatch<wl_callback::WlCallback, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.synced = true;
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ClientState {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for ClientState {
    fn event(
        _: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(ClientState: ignore wl_subsurface::WlSubsurface);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_buffer::WlBuffer);
delegate_noop!(ClientState: ignore xdg_positioner::XdgPositioner);
delegate_noop!(ClientState: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(ClientState: ignore xdg_popup::XdgPopup);
