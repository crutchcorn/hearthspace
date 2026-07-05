use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn install_signal_handler() -> Result<Arc<AtomicBool>, Box<dyn std::error::Error>> {
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let signal_shutdown = Arc::clone(&shutdown_requested);
    ctrlc::set_handler(move || {
        signal_shutdown.store(true, Ordering::SeqCst);
    })?;
    Ok(shutdown_requested)
}
