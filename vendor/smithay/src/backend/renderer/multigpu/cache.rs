use super::{ApiDevice, Dmabuf, DrmNode, ErasedContextId, GraphicsApi, MultiRenderer, Renderer};

/// A caller-owned intermediate for copying frames between two GPUs.
///
/// Use one cache per independently sized render target with
/// [`MultiRenderer::with_transfer_cache`]. At most one intermediate is retained,
/// including whether it uses direct sharing or the CPU-copy fallback. Changing
/// either device or renderer context discards it; size and format changes use
/// the same invalidation as the manager's default pair cache.
///
/// The caller must synchronize reuse: wait for the previous frame's target
/// completion before rendering into the intermediate again. If completion is
/// unavailable, clear the cache instead of reusing its storage. Clearing or
/// dropping a cache releases its references; it does not wait for GPU work.
/// [`super::GpuManager::invalidate_caches`] does not clear caller-owned caches.
#[derive(Debug, Default)]
pub struct TransferCache {
    binding: Option<Binding>,
    buffer: Option<(bool, Dmabuf)>,
}

#[derive(Debug, PartialEq, Eq)]
struct Binding {
    source: DrmNode,
    source_context: ErasedContextId,
    target: DrmNode,
    target_context: ErasedContextId,
}

impl TransferCache {
    /// Discard the cached intermediate and its device/context binding.
    pub fn clear(&mut self) {
        self.buffer = None;
        self.binding = None;
    }
}

impl<'render, 'target, R: GraphicsApi, T: GraphicsApi> MultiRenderer<'render, 'target, R, T> {
    /// Use a caller-owned intermediate instead of the manager's pair cache.
    ///
    /// This only changes where the intermediate is retained. Allocation,
    /// format negotiation, rendering, and CPU fallback are unchanged. The
    /// manager's default cache is left untouched. A single-device renderer
    /// does not use an intermediate and leaves this cache untouched as well.
    ///
    /// The caller must wait for the previous target completion before reuse,
    /// or clear the cache when no trustworthy completion is available. The
    /// exclusive borrow prevents two renderers from using the cache at once.
    pub fn with_transfer_cache(mut self, cache: &'target mut TransferCache) -> Self
    where
        <<R::Device as ApiDevice>::Renderer as super::RendererSuper>::TextureId: 'static,
        <<T::Device as ApiDevice>::Renderer as super::RendererSuper>::TextureId: 'static,
    {
        if let Some(target) = self.target.as_mut() {
            let binding = Binding {
                source: *self.render.node(),
                source_context: self.render.renderer().context_id().erased(),
                target: *target.device.node(),
                target_context: target.device.renderer().context_id().erased(),
            };
            if cache.binding.as_ref() != Some(&binding) {
                cache.clear();
                cache.binding = Some(binding);
            }
            target.cached_buffer = &mut cache.buffer;
        }
        self
    }
}
