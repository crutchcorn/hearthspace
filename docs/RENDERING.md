# Rendering & Client Buffers

Evergreen notes on how Hearthspace composites client buffers, how we handle
GPU (dmabuf) clients, and a known performance artifact that only appears under
software rendering in the development VM.

## How clients hand us pixels

Wayland clients deliver frames as either:

- **SHM buffers:** plain shared-memory pixels. The client has already finished
  drawing by the time it commits, so the buffer is immediately ready and cheap
  for us to upload/sample.
- **dmabuf buffers:** GPU-side buffers (e.g. GTK4's GL renderer, Firefox, the
  Xilem shell). These come with an *implicit-sync fence* that only signals
  once the client's GPU work has actually finished. We advertise
  `zwp_linux_dmabuf_v1` so these clients can hand us hardware buffers instead of
  failing EGL setup. See `run_winit` in `src/compositor/mod.rs`.

Because our `GlesRenderer` is not reachable from the Wayland handlers (it lives
on `CalloopData`, a sibling of the `App` handler state), dmabuf imports are
*deferred*: `DmabufHandler::dmabuf_imported` pushes the buffer onto
`App.pending_dmabuf_imports`, which the event loop drains and imports via
`process_pending_dmabuf_imports`.

## Deferring commits until buffers are ready (the readiness blocker)

A GPU client can commit a dmabuf before its fence has signalled. If we
composited it immediately we could sample a half-drawn buffer (tearing /
corruption). To avoid that, `CompositorHandler::new_surface` installs a
pre-commit hook on every surface that:

1. Reads the pending buffer and, if it is a dmabuf, calls
   `dmabuf.generate_blocker(Interest::READ)`.
2. Attaches a `DmabufBlocker` to the commit (`add_blocker`) and registers the
   one-shot `DmabufSource` (which polls the fence fd) on the calloop loop.
3. When the source fires, `blocker_cleared` releases the transaction and the
   commit is applied.

This is the standard Smithay/anvil pattern and mirrors how Mutter and KWin defer
compositing until client buffers are ready — the fence wait happens
asynchronously on the event loop instead of blocking it. `App` stores a
`LoopHandle<'static, CalloopData>` so the hook can register the fence source.

`generate_blocker` returns `AlreadyReady` (an `Err`) when the fence is already
signalled, which is the common steady-state case; we then skip the blocker and
apply the commit immediately.

## Known artifact: ~1.5s stall on the first frame of a GPU client (lavapipe VM only)

### Symptom

Launching a GPU/dmabuf client (GNOME Calculator, Firefox, the Xilem shell)
freezes the whole shell for ~1.5 seconds **once per client**. Input typed during
the freeze is buffered and applied late (e.g. typing "firefox" shows up as
`fffffffffffffffffi`). SHM clients (e.g. `foot`) do **not** trigger it.

### Root cause (verified by instrumentation)

The stall is entirely inside `backend.submit()` (`eglSwapBuffers`), and it is
**not** the client-fence wait:

- With the readiness blocker installed, the slow submit happens on the *import*
  frame — while the client's content commit is still deferred by the blocker.
- When the content actually composites (after the blocker clears), that submit
  is fast.

So the cost is **our own GL context realizing the freshly-imported dmabuf at
swap time**, paid once per GPU client. Under lavapipe (llvmpipe + virtio-gpu)
there is no real GPU timeline, so the "GPU" work runs synchronously on the
single-threaded event loop. A decisive experiment confirmed this: inserting a
3-second `sleep` after the import (before compositing) dropped the subsequent
submit from ~1.5s to ~92µs, because the software driver finished the work in the
background during the sleep.

### Why this is a dev-VM artifact, not a real bug

On real hardware, dmabuf import is a zero-copy `EGLImage` bind and sampling is
asynchronous on the GPU, so this freeze should not occur. The ~1.5s is the
classic llvmpipe + virtio-gpu software path doing a CPU copy/detile of the
client buffer. It is specific to the nested development VM, which uses software
rendering (lavapipe) over a virtio-GPU render node.

### Why we are not "fixing" it with a render thread

Moving compositing to a dedicated CPU render thread fights Smithay's design:

- `GlesRenderer` is explicitly `!Send` (`_not_send: PhantomData<*mut ()>`), so
  the existing renderer cannot be moved to another thread. A render thread would
  have to bypass `winit::init`, build the EGL context/surface by hand from the
  raw winit window (both `EGLContext` and `EGLSurface` *are* `Send`), and
  construct the renderer on the render thread.
- Render elements (`WaylandSurfaceRenderElement`) are tied to both the
  renderer's textures and live Wayland surface state on the main thread, so the
  scene would have to be snapshotted into `Send` data every frame.

None of Smithay's own backends (winit, udev) use a CPU render thread — their
parallelism *is* the GPU (async KMS/GL submit), which does not exist under
lavapipe. The real fix is real hardware (or the native DRM backend, see
[DRM.md](./DRM.md)), where the work is offloaded to the GPU. The readiness
blocker above is already the correct real-hardware behavior; we accept the
one-time-per-client stall as a known limitation of the software-rendered VM.
