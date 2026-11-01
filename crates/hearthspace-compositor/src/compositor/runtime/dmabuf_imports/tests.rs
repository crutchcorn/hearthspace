use std::{cell::Cell, io};

use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::process;
use fixture::{Fixture, marker};

mod client;
mod fixture;

struct RendererLease<'a>(&'a Cell<usize>);

impl Drop for RendererLease<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn create_and_create_immed_survive_unavailable_and_failed_acquisition_then_import_once() {
    let mut f = Fixture::new();
    assert!(f.submit(1, false).is_none());
    let immediate = f.submit(2, true).unwrap();
    f.assert_deferred(&[1, 2]);

    // Seat pause and active-seat/no-canonical-renderer both produce None. They
    // must preserve the real App queue, not consume/fail its protocol notifiers.
    for _ in 0..2 {
        process(
            &mut f.app,
            &mut f.full_redraw,
            || Ok(None::<()>),
            |_, _| -> Result<(), io::Error> { panic!("no renderer acquired") },
        );
        f.assert_deferred(&[1, 2]);
    }

    // A selected canonical node is not sufficient: manager acquisition can
    // still fail after libseat has resumed. Repeated errors must remain pending.
    let acquisitions = Cell::new(0);
    for _ in 0..2 {
        process(
            &mut f.app,
            &mut f.full_redraw,
            || -> Result<Option<()>, Box<dyn std::error::Error>> {
                acquisitions.set(acquisitions.get() + 1);
                Err(io::Error::other("canonical renderer acquisition failed").into())
            },
            |_, _| -> Result<(), io::Error> { panic!("acquisition failed") },
        );
        f.assert_deferred(&[1, 2]);
    }
    assert_eq!(acquisitions.get(), 2);

    // Failed secondary output readiness must not block a healthy canonical
    // import. Only the acquisition result controls this production drain.
    #[cfg(feature = "udev")]
    f.app.set_native_lock_outputs_available(false);
    let releases = Cell::new(0);
    let mut imported = Vec::new();
    process(
        &mut f.app,
        &mut f.full_redraw,
        || {
            acquisitions.set(acquisitions.get() + 1);
            Ok(Some(RendererLease(&releases)))
        },
        |lease, buffer| -> Result<(), io::Error> {
            assert_eq!(lease.0.get(), 0, "renderer was released before import");
            imported.push(marker(buffer));
            Ok(())
        },
    );
    assert_eq!(acquisitions.get(), 3, "one acquisition for the whole batch");
    assert_eq!(releases.get(), 1);
    assert_eq!(imported, [1, 2]);
    assert!(f.app.pending_dmabuf_imports.is_empty());
    assert_eq!(f.full_redraw, 1);
    f.sync();
    assert_eq!(f.events.created.len(), 1);
    assert_eq!(f.events.created[0].0, 1);
    assert!(f.events.failed.is_empty());

    f.full_redraw = 0;
    process(
        &mut f.app,
        &mut f.full_redraw,
        || -> Result<Option<()>, Box<dyn std::error::Error>> {
            panic!("empty queue must not acquire again")
        },
        |_, _| -> Result<(), io::Error> { panic!("request imported twice") },
    );
    assert_eq!(f.full_redraw, 0);
    f.use_buffer(&immediate);
    let created = f.events.created[0].1.clone();
    f.use_buffer(&created);
    assert_eq!(f.events.created.len(), 1);
    assert!(f.events.failed.is_empty());
}

#[test]
fn acquired_renderer_rejection_fails_create_without_disconnect_or_duplicate_response() {
    let mut f = Fixture::new();
    assert!(f.submit(3, false).is_none());
    let mut imported = Vec::new();
    f.full_redraw = 2;
    process(
        &mut f.app,
        &mut f.full_redraw,
        || Ok(Some(())),
        |_, buffer| {
            imported.push(marker(buffer));
            Err(io::Error::other("raw GLES rejected this buffer"))
        },
    );
    f.sync();
    assert_eq!(imported, [3]);
    assert!(f.app.pending_dmabuf_imports.is_empty());
    assert!(f.events.created.is_empty());
    assert_eq!(f.events.failed, [3]);
    assert_eq!(
        f.full_redraw, 2,
        "existing forced redraw count is preserved"
    );
    process(
        &mut f.app,
        &mut f.full_redraw,
        || -> Result<Option<()>, Box<dyn std::error::Error>> {
            panic!("rejected request must not retry")
        },
        |_, _| -> Result<(), io::Error> { panic!("request imported twice") },
    );
    f.sync();
    assert_eq!(f.events.failed, [3]);
}

#[test]
fn acquired_renderer_rejection_keeps_create_immed_protocol_error() {
    let mut f = Fixture::new();
    let _buffer = f.submit(4, true).unwrap();
    let mut imported = Vec::new();
    process(
        &mut f.app,
        &mut f.full_redraw,
        || Ok(Some(())),
        |_, buffer| {
            imported.push(marker(buffer));
            Err(io::Error::other("raw GLES rejected this buffer"))
        },
    );
    assert_eq!(imported, [4]);
    assert!(f.app.pending_dmabuf_imports.is_empty());
    assert_eq!(f.full_redraw, 1);
    // failed() already posted and flushed the fatal protocol error. Read it
    // without sending another request to a connection the server has killed.
    assert!(f.connection.prepare_read().unwrap().read().is_err());
    let error = f.connection.protocol_error().unwrap();
    assert_eq!(error.object_interface, "zwp_linux_buffer_params_v1");
    assert_eq!(
        error.code,
        zwp_linux_buffer_params_v1::Error::InvalidWlBuffer as u32
    );
}
