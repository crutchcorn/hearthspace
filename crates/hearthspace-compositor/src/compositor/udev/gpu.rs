use std::collections::HashMap;

use smithay::backend::{
    allocator::Format,
    drm::{DrmDeviceFd, DrmNode},
    renderer::{
        ImportDma,
        gles::GlesRenderer,
        multigpu::{GpuManager, gbm::GbmGlesBackend},
        sync::SyncPoint,
    },
};
use tracing::{info, warn};

mod feedback;

pub(super) type GpuApi = GbmGlesBackend<GlesRenderer, DrmDeviceFd>;

pub(super) struct GpuState {
    pub(super) manager: GpuManager<GpuApi>,
    pub(super) scene_node: Option<DrmNode>,
    // Smithay caches one intermediate per GPU pair, shared by that pair's
    // outputs. Wait for the last target read before rewriting it.
    pub(super) transfers: HashMap<(DrmNode, DrmNode), SyncPoint>,
    feedback_dirty: bool,
    feedback: feedback::FeedbackState,
}

impl GpuState {
    pub(super) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            manager: GpuManager::new(GpuApi::default())?,
            scene_node: None,
            transfers: HashMap::new(),
            feedback_dirty: false,
            feedback: feedback::FeedbackState::default(),
        })
    }

    pub(super) fn select_scene(&mut self, node: Option<DrmNode>) {
        if self.scene_node != node {
            info!(previous = ?self.scene_node, next = ?node, "canonical scene GPU changed");
            self.scene_node = node;
            self.feedback_dirty = true;
            self.transfers.clear();
        }
    }

    pub(super) fn refresh_app(&mut self, state: &mut crate::compositor::App) {
        if !self.feedback_dirty {
            return;
        }
        state.background_image_tile = None;
        state.corner_blur = Default::default();
        state.window_backdrop = Default::default();
        state.window_frames = Default::default();
        let formats = self.scene_node.and_then(|node| {
            self.manager.single_renderer(&node).ok().map(|renderer| {
                (
                    node.dev_id(),
                    renderer
                        .as_ref()
                        .dmabuf_formats()
                        .into_iter()
                        .collect::<Vec<Format>>(),
                )
            })
        });
        let Some((node, formats)) = formats else {
            // Keep the old global for existing resources; import requests stay
            // deferred while no canonical renderer is available.
            return;
        };
        match self.feedback.refresh::<crate::compositor::App>(
            &state.display_handle,
            &mut state.dmabuf_state,
            &mut state._dmabuf_global,
            &mut state.dmabuf_feedback_surfaces,
            node,
            formats,
        ) {
            Ok(()) => self.feedback_dirty = false,
            Err(error) => warn!(%error, "failed to refresh canonical GPU dmabuf feedback"),
        }
    }
}
