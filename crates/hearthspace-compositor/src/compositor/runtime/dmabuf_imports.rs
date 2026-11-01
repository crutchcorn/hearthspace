//! Keep asynchronous client requests until their renderer can be acquired.
use std::{error::Error, fmt::Display};

use smithay::backend::allocator::{Buffer, dmabuf::Dmabuf};
use tracing::{debug, trace, warn};

use crate::compositor::App;

/// Acquisition and the import operation are the only external GPU boundaries.
/// Keep the acquired renderer alive for the whole drain, with no probe/reacquire
/// gap. In particular, temporary unavailability must not fail create_immed and
/// disconnect an otherwise valid client.
pub(super) fn process<R, E: Display>(
    state: &mut App,
    full_redraw: &mut u8,
    acquire: impl FnOnce() -> Result<Option<R>, Box<dyn Error>>,
    mut import: impl FnMut(&mut R, &Dmabuf) -> Result<(), E>,
) {
    if state.pending_dmabuf_imports.is_empty() {
        return;
    }
    let mut renderer = match acquire() {
        Ok(Some(renderer)) => renderer,
        Ok(None) => return,
        Err(error) => {
            trace!(%error, "deferring client dmabuf imports until renderer acquisition succeeds");
            return;
        }
    };
    debug!(
        count = state.pending_dmabuf_imports.len(),
        "processing pending client dmabuf imports"
    );
    for (dmabuf, notifier) in state.pending_dmabuf_imports.drain(..) {
        match import(&mut renderer, &dmabuf) {
            Ok(()) => {
                let _ = notifier.successful::<App>();
            }
            Err(error) => {
                warn!(%error, size = ?dmabuf.size(), format = ?dmabuf.format(),
                    planes = dmabuf.num_planes(), has_modifier = dmabuf.has_modifier(),
                    "failed to import client dmabuf");
                notifier.failed();
            }
        }
    }
    // Importing can leave EGL surfaceless. Force the next redraw, but do not
    // change redraw state when acquisition left every request pending.
    *full_redraw = (*full_redraw).max(1);
}

#[cfg(test)]
mod tests;
