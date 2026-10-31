use smithay::backend::renderer::{Frame, Renderer, multigpu::TransferCache, sync::SyncPoint};

/// One live output owns both the storage and the dependency for rewriting it.
#[derive(Debug, Default)]
pub(crate) struct OutputTransfer {
    pub(crate) cache: TransferCache,
    pub(crate) completion: Completion,
}

impl OutputTransfer {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Call before rewriting either the persistent scene or its intermediate.
    /// A failed wait retains the fence and prevents that rewrite. Missing
    /// completion must never make previously used storage reusable.
    pub(crate) fn prepare<R: Renderer>(&mut self, renderer: &mut R) -> Result<(), R::Error> {
        if let Some(sync) = self.completion.0.as_ref() {
            renderer.wait(sync)?;
        } else {
            self.cache.clear();
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct Completion(Option<SyncPoint>);

impl Completion {
    /// Retain completion before propagating a draw error or attempting readback
    /// and KMS submission. Those later failures do not undo submitted GPU work.
    /// An error here has no trustworthy fence; the caller must reset the cache.
    pub(crate) fn finish<F: Frame>(
        &mut self,
        mut frame: F,
        draw: impl FnOnce(&mut F) -> Result<(), F::Error>,
    ) -> Result<(Result<(), F::Error>, SyncPoint), F::Error> {
        let drawn = draw(&mut frame);
        // Always finish explicitly, including partially submitted failed draws.
        self.0 = None;
        let sync = frame.finish()?;
        self.0 = Some(sync.clone());
        Ok((drawn, sync))
    }
}
