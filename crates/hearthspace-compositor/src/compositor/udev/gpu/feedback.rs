//! Keep allocation advice and Smithay's creation validation in sync on failover.
use smithay::{
    backend::allocator::Format,
    wayland::{
        buffer::BufferHandler,
        compositor::with_states,
        dmabuf::{
            DmabufFeedbackBuilder, DmabufGlobal, DmabufGlobalData, DmabufHandler, DmabufState,
            SurfaceDmabufFeedbackState,
        },
    },
};
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_server::{DisplayHandle, GlobalDispatch, Weak, protocol::wl_surface::WlSurface};

#[derive(Default)]
pub(super) struct FeedbackState {
    initialized: bool,
}

impl FeedbackState {
    pub(super) fn refresh<D>(
        &mut self,
        display: &DisplayHandle,
        state: &mut DmabufState,
        global: &mut DmabufGlobal,
        surfaces: &mut Vec<Weak<WlSurface>>,
        device: u64,
        formats: Vec<Format>,
    ) -> std::io::Result<()>
    where
        D: GlobalDispatch<ZwpLinuxDmabufV1, DmabufGlobalData>
            + BufferHandler
            + DmabufHandler
            + 'static,
    {
        // Smithay drops an empty main tranche, but the protocol requires one.
        // Reject before changing globals or surface state; the GPU owner keeps
        // feedback_dirty set on Err and retries when import formats recover.
        if formats.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "canonical GPU has no importable dmabuf formats",
            ));
        }
        let feedback = DmabufFeedbackBuilder::new(device, formats).build()?;
        if self.initialized {
            // The vendored Smithay repair extends creation validation for old
            // bindings/params before publishing these new allocation formats.
            state.set_default_feedback(global, &feedback);
        } else {
            // Startup without a GPU used v3. Updating its default cannot add
            // feedback requests; replace the advertised global just once.
            state.disable_global::<D>(display, global);
            *global = state.create_global_with_default_feedback::<D>(display, &feedback);
            self.initialized = true;
        }

        // Registration happens at the first feedback request, independent of
        // window roles or mapping. This includes every requesting subsurface,
        // popup, cursor, lock surface and still-roleless client surface.
        surfaces.retain(|surface| {
            let Ok(surface) = surface.upgrade() else {
                return false;
            };
            with_states(&surface, |states| {
                if let Some(surface_feedback) = SurfaceDmabufFeedbackState::from_states(states) {
                    surface_feedback.set_feedback(&feedback);
                }
            });
            true
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
