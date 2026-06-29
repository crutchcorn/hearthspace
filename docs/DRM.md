# DRM/KMS Backend Notes

Evergreen reference for Hearthspace's display backends. This explains how the
native DRM/KMS path differs from the nested winit path, what state is shared, and
which native-backend constraints are intentional.

## Nested vs Native

Hearthspace can run three display backends:

- **winit:** nested development backend. Hearthspace is a Wayland/X11 client in
  an existing desktop session. This is the default everyday path.
- **headless:** offscreen backend for command-socket screenshots and e2e tests.
- **udev/DRM/KMS:** native backend. Hearthspace owns a VT/session, reads input
  through libinput, renders into GBM buffers, and presents directly with KMS.

| Concern        | winit (nested)                       | udev/DRM/KMS (native)                    |
| -------------- | ------------------------------------ | ---------------------------------------- |
| Where it runs  | A window inside another compositor   | Bare VT/session, owns the display        |
| Display output | Host compositor presents the window  | Direct KMS commit to a connector/CRTC    |
| Frame timing   | Host decides; winit is calloop-driven | DRM vblank/page-flip events pace redraws |
| Input          | winit translates host input          | libinput reads evdev through session fds |
| Buffers        | winit surface/framebuffer            | GBM buffers exported as dmabufs          |
| Screenshots    | Renderer readback                    | Unsupported until native readback exists |

## Runtime Selection

Backend selection is explicit when requested and auto-detected otherwise:

```text
--headless -> headless
--winit    -> winit
--tty      -> udev/DRM
auto       -> winit when WAYLAND_DISPLAY or DISPLAY is set, otherwise udev
```

Native development commands:

```sh
cargo run --features udev -- --tty
cargo run --no-default-features --features udev -- --tty --no-shell
```

See [NATIVE_TESTING.md](./NATIVE_TESTING.md) for VT smoke commands, log capture,
and native failure triage.

Useful native safety exits:

- `Ctrl+Alt+Backspace`
- `Ctrl+Alt+Esc`
- `SIGINT` / `SIGTERM`
- `--exit-after-ms INTEGER` for timed VT smoke tests

## Cargo Features

The default feature set keeps native-only dependencies out of everyday builds:

```toml
[features]
default = ["winit"]
winit = ["smithay/backend_winit"]
udev = [
  "smithay/backend_drm",
  "smithay/backend_gbm",
  "smithay/backend_libinput",
  "smithay/backend_session_libseat",
  "smithay/backend_udev",
  "smithay/renderer_multi",
]
```

Shared Smithay features remain on the dependency itself: `desktop`,
`renderer_gl`, and `wayland_frontend`.

`renderer_multi` is enabled for native dmabuf import plumbing, but the first
native milestone still opens and renders through a single primary DRM device.

## Event Loop Model

All backends share the same calloop-driven compositor loop:

- Wayland display fd dispatches client requests.
- The shell command socket accepts control commands.
- Backend event sources drive input/redraw timing.
- Post-dispatch maintenance imports pending dmabufs, advances idle/viewport
  state, applies cursor changes, renders if needed, flushes clients, and cleans
  popups/outputs.

The native backend adds fd-driven sources for:

- `LibSeatSessionNotifier` for VT/session pause and activation.
- `UdevBackend` for DRM device add/change/remove events.
- `LibinputInputBackend` for keyboard/pointer events.
- `DrmDeviceNotifier` for page-flip/vblank completion.

The native render path is vblank-paced. `App::request_redraw` marks state dirty;
if a KMS page flip is pending, the request is deferred until the vblank handler
clears the pending frame. Native animation pacing also wakes from vblank rather
than a timer.

## Native Backend Architecture

Native setup currently uses one selected primary DRM device:

1. Acquire a `LibSeatSession` and seat name.
2. Enumerate DRM devices through Smithay's udev backend.
3. Select `primary_gpu(seat)` and log secondary DRM devices as ignored for now.
4. Open the selected DRM node through the session.
5. Build explicit ownership records:
   - `RenderNode`: DRM fd plus `GlesRenderer`.
   - `ScanoutNode`: DRM fd plus `DrmDevice`.
   - `KmsOutputSurface`: selected connector/CRTC/mode plus `GbmBufferedSurface`.
6. Enumerate connected connectors, choose the first usable target, and create a
   Smithay `Output` from connector metadata.
7. Create a GBM allocator/device and EGL/GLES renderer backed by the DRM node.
8. Render with the shared `App::render_frame` into GBM buffers and queue KMS
   commits through `GbmBufferedSurface`.

The render and scanout nodes are structurally separate even though they point at
the same DRM node today. That keeps the code ready for future render-node,
scanout-node, and per-output allocator ownership without enabling multi-GPU
behavior prematurely.

## Output State

The compositor tracks outputs as an `OutputSet`:

- One primary output backs the current KMS render target.
- Secondary connected connectors are advertised as Smithay `Output`s with
  Wayland globals, but are not rendered to yet.
- Secondary outputs are laid out horizontally after the primary in connector
  enumeration order.
- Existing secondary outputs update their mode/scale/location on connector
  resync.
- Removed secondary outputs have their Wayland globals disabled.
- Absolute pointer mapping clamps against the aggregate logical output bounds.

Primary-output rebuild after the selected KMS target changes is still future
work. The first connected target remains the rendered output for now.

## Dmabuf Feedback And Clients

Native dmabuf feedback uses the opened DRM node's `dev_t` as the main device.
The native renderer advertises its dmabuf import formats, and client dmabufs are
imported through the udev `GlesRenderer` before the next full redraw.

Import failures should include enough detail to debug Mesa/GTK/client issues:
size, format, plane count, modifier presence, and node.

The compositor keeps running after recoverable live errors from client dispatch,
client flush, frame callbacks, or render submission. Those errors are logged and
the next redraw is forced full where appropriate. Initialization failures remain
fatal.

## KMS Damage Clips

Renderer-side damage tracking remains enabled and useful. It minimizes GLES
redraw work into the GBM buffer.

Native KMS commits intentionally do **not** pass scanout damage clips today.
During Firefox/GNOME Calculator testing on the Parallels/virgl VM, forwarding
damage through Smithay's `GbmBufferedSurface::queue_buffer` produced repeated KMS
commit failures:

```text
Page flip commit failed on device Some("/dev/dri/card1") (Invalid argument (os error 22))
```

The likely failing path is Smithay converting renderer damage into
`PlaneDamageClips` / `FB_DAMAGE_CLIPS` and attaching that blob to the primary
plane commit. Until we have per-device fallback, native commits pass `None` for
KMS damage clips and submit full-plane scanout updates.

Future re-enablement should be per output/device: try clips, retry the same
frame once without clips on `EINVAL`, then disable KMS damage clips for that
output/device for the rest of the run.

## What Stays Shared

Almost all compositor logic is backend-neutral:

- Wayland protocol handlers.
- Window management and hit-testing.
- Viewport geometry and animations.
- Idle daemon/window activity tracking.
- Shell command socket.
- Xilem shell integration.
- Shared `App::render_frame` over `GlesRenderer`.

Backend-specific code owns output creation, frame scheduling, renderer/buffer
acquisition, dmabuf main-device discovery, and input/event sources.

## Reference

Smithay's reference compositor anvil implements winit, x11, and udev/DRM
backends over one shared compositor state and remains the canonical model for
this structure.
