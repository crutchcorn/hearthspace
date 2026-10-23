use xilem::{
    ViewCtx,
    core::{NoElement, View},
    tokio::sync::{oneshot, watch},
    view::task_raw,
};

use super::{Request, Response};
use crate::{background_thumbnail::ThumbnailCache, state::SettingsApp};

/// Keep one worker above the category switcher so repeated visits reuse its cache.
pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy, state: &mut SettingsApp| {
            state
                .background_preview
                .synchronize_path(state.settings.background_image_path.as_deref());
            let (sender, receiver) = watch::channel(state.background_preview.request.clone());
            state.background_preview.sender = Some(sender);
            run(receiver, move |response| proxy.message(response).is_ok())
        },
        |state: &mut SettingsApp, response| state.background_preview.receive(response),
    )
}

async fn run(mut receiver: watch::Receiver<Request>, mut publish: impl FnMut(Response) -> bool) {
    let mut cache = ThumbnailCache::new();
    loop {
        let request = receiver.borrow_and_update().clone();
        let current = request.clone();
        let loaded = if let Some(path) = current.path {
            blocking_load(move || {
                let image = cache.load(path.as_ref());
                (cache, image)
            })
            .await
        } else {
            Some((cache, None))
        };
        let (next_cache, image) = loaded.unwrap_or_else(|| (ThumbnailCache::new(), None));
        cache = next_cache;
        if !publish(Response { request, image }) || receiver.changed().await.is_err() {
            break;
        }
    }
}

async fn blocking_load<T: Send + 'static>(load: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = oneshot::channel();
    // Tokio waits for spawn_blocking work during runtime shutdown. A separate
    // thread lets Settings close immediately during a slow read/decode. The
    // task awaits every job, so only one runs; only shutdown detaches it.
    let thread = std::thread::Builder::new()
        .name("background-thumbnail".into())
        .spawn(move || {
            let _ = sender.send(load());
        })
        .ok()?;
    let result = receiver.await.ok();
    // The load has completed (or panicked), so joining only finishes thread
    // cleanup. A panic/error simply leaves the dotted fallback.
    let _ = thread.join();
    result
}

#[cfg(test)]
mod tests {
    use super::blocking_load;
    use std::{sync::mpsc, time::Duration};
    use xilem::tokio::{runtime::Builder, sync::oneshot};

    #[test]
    fn closing_the_async_worker_does_not_wait_for_an_active_image_load() {
        let (release, wait_for_release) = mpsc::channel();
        let (closed, wait_for_close) = mpsc::channel();
        let runtime_thread = std::thread::spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(async {
                let (started, wait_for_start) = oneshot::channel();
                let job = xilem::tokio::spawn(blocking_load(move || {
                    started.send(()).unwrap();
                    wait_for_release.recv().unwrap();
                }));
                wait_for_start.await.unwrap();
                job.abort();
                let _ = job.await;
            });
            drop(runtime);
            closed.send(()).unwrap();
        });
        let closed_without_waiting = wait_for_close.recv_timeout(Duration::from_secs(2)).is_ok();
        // Release the loader even if the assertion fails, so the regression
        // cannot leave a blocked thread behind when run against spawn_blocking.
        release.send(()).unwrap();
        runtime_thread.join().unwrap();
        assert!(
            closed_without_waiting,
            "closing Settings must not wait for decoding"
        );
    }

    #[test]
    fn a_panicking_loader_settles_without_stopping_the_async_worker() {
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            assert!(
                blocking_load::<()>(|| panic!("failed decoder"))
                    .await
                    .is_none()
            );
            assert_eq!(blocking_load(|| 42).await, Some(42));
        });
    }
}
