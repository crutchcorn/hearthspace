use std::os::unix::io::AsFd;

use super::{
    Allocator, Buffer, DrmCompositor, ExportFramebuffer, FrameState, Framebuffer, PreparedFrameKind,
};

impl<A, F, U, G> DrmCompositor<A, F, U, G>
where
    A: Allocator,
    F: ExportFramebuffer<A::Buffer>,
    F::Framebuffer: std::fmt::Debug + Send + Sync + 'static,
    G: AsFd + 'static,
{
    /// Enable or disable KMS framebuffer damage clips. Enabled by default.
    ///
    /// This controls only `FB_DAMAGE_CLIPS`. Renderer damage tracking, buffer
    /// ages, and rendering and scan-out synchronization are unchanged. The
    /// setting persists across buffer resets, state resets, and [`Self::clear`].
    ///
    /// Disabling discards clips in all retained frames and requires a full plane
    /// update on the next successful submission, even if the scene is unchanged.
    /// The atomic backend explicitly clears `FB_DAMAGE_CLIPS` for planes without
    /// clips. An already submitted frame cannot be changed; its retained state is
    /// cleared so completing or reusing it cannot restore old clips.
    ///
    /// This method does not submit a frame. The caller must schedule rendering
    /// and submission, or submit an already prepared frame. Re-enabling permits
    /// future frames to generate clips; it does not recreate discarded clips.
    pub fn set_damage_clips_enabled(&mut self, enabled: bool) {
        if self.damage_clips_enabled == enabled {
            return;
        }
        self.damage_clips_enabled = enabled;
        if enabled {
            return;
        }

        self.current_frame.clear_damage_clips();
        if let Some(pending) = self.pending_frame.as_mut() {
            pending.frame.clear_damage_clips();
        }
        if let Some(queued) = self.queued_frame.as_mut() {
            queued.prepared_frame.frame.clear_damage_clips();
            queued.prepared_frame.kind = PreparedFrameKind::Full;
        }
        if let Some(next) = self.next_frame.as_mut() {
            next.frame.clear_damage_clips();
            next.kind = PreparedFrameKind::Full;
        }

        // Force an unchanged scene through render_frame's existing full-update
        // path. handle_flip clears this only after a successful full submission,
        // so failures cannot consume the required KMS property reset.
        self.reset_pending = true;
    }
}

impl<B: Buffer, F: Framebuffer> FrameState<B, F> {
    fn clear_damage_clips(&mut self) {
        for (_, state) in &mut self.planes {
            if let Some(config) = state.config.as_mut() {
                config.damage_clips = None;
            }
        }
    }
}
