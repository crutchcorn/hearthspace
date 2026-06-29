# UDEV/DRM Backend: Step-by-Step Plan

Implementation checklist for Hearthspace's native DRM/KMS backend. This is the
action plan that follows from [../docs/DRM.md](../docs/DRM.md); keep the DRM doc
as the evergreen architecture/background reference and this file as the working
todo list.

The goal is a native backend that shares the existing compositor state and draw
path with the nested `winit` backend and the headless test backend.

## Current Baseline

- [x] `src/compositor/mod.rs` already uses `calloop` for Wayland client I/O,
      the shell command socket, winit input/window events, and redraw dispatch.
- [x] `Backend` already separates backend-owned rendering resources for `Winit`
      and `Headless`.
- [x] `App::render_frame` in `src/compositor/rendering.rs` is renderer-generic
      and only needs a `GlesRenderer`, framebuffer, and `OutputDamageTracker`.
- [x] `Headless` provides a useful non-winit backend reference for screenshots,
      fixed-size output setup, and offscreen rendering.
- [x] Dmabuf setup, Wayland source setup, command socket setup, and the main
      dispatch loop are shared between `run_winit` and `run_headless`.
- [x] Output creation remains backend-specific for initial backend setup; dynamic
      native output creation starts from the Step 8 hotplug work.
- [x] `App` now wraps its single output in a `PrimaryOutput` abstraction, which is
      enough for the first native milestone and can grow into an output set.

## Step 1: Add Feature Gates And CLI Selection

Goal: compile the current development path by default while allowing a native
build to opt into the heavier DRM stack.

- [x] Move Smithay backend features out of the dependency line and into crate
      features.

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

- [x] Keep shared Smithay features on the dependency itself:
      `desktop`, `renderer_gl`, and `wayland_frontend`.
- [x] Gate winit imports and `run_winit` with `#[cfg(feature = "winit")]`.
- [x] Gate or genericize `src/compositor/input.rs`, which currently imports
      `smithay::backend::winit::WinitInput` directly.
- [x] Add crate-level `winit` and `udev` feature names for startup/backend cfgs.
- [x] Add `--tty` and `--winit` constants in `src/config.rs`.
- [x] Extend `RunOptions` with a backend selection enum instead of more booleans:
      `Auto`, `Winit`, `Udev`, `Headless`.
- [x] Select the backend in `main.rs` using:
      `--headless` -> headless, `--winit` -> winit, `--tty` -> udev, otherwise
      nested when `WAYLAND_DISPLAY` or `DISPLAY` exists, native otherwise.
- [x] Reject conflicting explicit backend flags.
- [x] Return a clear error when a selected backend was not compiled in, for
      example `--tty requires rebuilding with --features udev`.
- [x] Return a clear placeholder error when `--features udev -- --tty` reaches
      the selected-but-not-implemented native backend.

Done when: `cargo check`, `cargo check --no-default-features --features udev`,
and `cargo check --features udev` reach backend-selection code without feature
resolution errors.

## Step 2: Extract Shared Compositor Bootstrap

Goal: avoid copying all of `run_winit` again for `run_udev`.

- [x] Introduce a small shared initializer that receives the backend-created
      Wayland display and creates Smithay globals, seat, pointer, keyboard, app
      catalog, popup manager, and event loop handle.
- [x] Keep backend-specific values as parameters: initial output metadata, output
      size, scale, dmabuf formats, dmabuf main device, and command socket label.
- [x] Extract dmabuf global creation into one helper that takes renderer formats
      and an optional render-node `dev_t`.
- [x] Extract Wayland listening socket registration into one helper.
- [x] Extract Wayland display fd dispatch registration into one helper.
- [x] Extract shell command socket creation/registration into one helper.
- [x] Extract the common post-dispatch maintenance loop:
      pending dmabuf imports, idle transitions, viewport animation, cursor
      application, redraw check, client flush, popup cleanup, output cleanup.
- [x] Leave `run_winit` and `run_headless` behavior unchanged after the refactor.

Done when: `run_winit` and `run_headless` are thin backend setup functions plus
the shared dispatch loop, with no behavior changes in headless E2E tests.

## Step 3: Add A UDEV Backend Skeleton

Goal: create a feature-gated module that can acquire a seat and enumerate GPUs,
without modesetting yet.

- [x] Add `src/compositor/udev.rs` behind `#[cfg(feature = "udev")]`.
- [x] Add `Backend::Udev(Box<udev::UdevBackendState>)` behind the same cfg.
- [x] In `run_udev`, create a `LibSeatSession` and insert its
      `LibSeatSessionNotifier` into the calloop loop.
- [x] On `SessionEvent::PauseSession`, stop scheduling new DRM commits, mark
      placeholder KMS device state inactive, and leave Wayland clients connected.
- [x] On `SessionEvent::ActivateSession`, reactivate placeholder device state,
      re-scan DRM devices, queue connector re-scan, and queue repaint for the
      future KMS output path.
- [x] Create `smithay::backend::udev::UdevBackend::new(seat_name)` and process the
      initial `device_list()` before inserting it into the loop.
- [x] Insert the `UdevBackend` source and log `Added`, `Changed`, and `Removed`
      events with the device id/path.
- [x] Use `session.open(...)` for DRM nodes instead of opening them directly.
- [x] For the first milestone, choose one primary GPU and ignore secondary GPUs
      with an explicit log message.

Done when: `cargo run --no-default-features --features udev -- --tty --no-shell`
can start on a VT, acquire a session, list DRM devices, respond to VT switch
pause/activate, then exit cleanly without modesetting.

## Step 4: Wire Libinput Input

Goal: feed native evdev input into the existing Smithay seat.

- [x] Create a `libinput::Libinput` with
      `LibinputSessionInterface::from(session)` after opening the primary DRM
      node.
- [x] Call `udev_assign_seat(seat_name)` before wrapping it in
      `LibinputInputBackend::new`.
- [x] Insert `LibinputInputBackend` into the calloop loop.
- [x] Make `handle_input_event` generic over Smithay input backends instead of
      accepting only `InputEvent<WinitInput>`.
- [x] Make the axis-frame helpers generic too; they currently take
      `PointerAxisEvent<WinitInput>`.
- [x] Add relative pointer motion handling to the generic input path, since
      libinput mice usually report relative motion rather than absolute motion.
- [x] Reuse the generic `handle_input_event(&mut App, event)` for keyboard,
      pointer button, relative motion, absolute motion, axis, and gesture events
      where Smithay's event traits line up with winit/libinput backends.
- [x] Map absolute pointer events into the active output's logical geometry.
- [x] Ignore unsupported tablet/touch/switch events initially with concise logs,
      then add follow-up todos when concrete hardware needs them.
- [x] On session pause, ensure libinput devices are suspended or their events are
      ignored until activation.

Done when: on a native VT, pointer movement, clicks, keyboard focus, key repeats,
and scroll zoom behave like the nested backend.

## Step 5: Bring Up One KMS Output

Goal: modeset one connected monitor and present a first Hearthspace frame.

- [x] For the selected DRM node, create a `DrmDevice` and insert its
      `DrmDeviceNotifier` into calloop.
- [x] Enumerate connectors, filter connected connectors with modes, and choose a
      preferred/first mode for logging.
- [x] Pick a compatible CRTC candidate for the first connected connector.
- [x] Let Smithay pick and claim a primary plane for the selected CRTC through
      `DrmDevice::create_surface`.
- [x] Create a `DrmSurface` for that connector/CRTC/mode.
- [x] Create a GBM device/allocator for the DRM fd.
- [x] Create an EGL/GLES renderer backed by GBM rather than a winit surface or
      surfaceless display.
- [x] Create a Smithay `Output` using connector metadata instead of the current
      hardcoded `hearthspace-0`/`Nested Canvas` values.
- [x] Use the chosen mode's pixel size and refresh for `OutputDamageTracker` and
      Wayland output state.
- [x] Render one full frame with `App::render_frame` into a GBM-backed buffer and
      commit it to the `DrmSurface`.
- [x] Keep software/headless screenshots unsupported for `Backend::Udev` at this
      step unless readback is trivial; return a clear command-socket error.

Done when: running with `--tty --no-shell` modesets one monitor and displays the
background/shell-less compositor frame without clients.

## Step 6: Page-Flip Driven Rendering

Goal: redraw only when KMS can accept the next frame.

- [x] Track initial single-output native state: `DrmSurface`, `Output`, damage
      tracker, renderer/render target, pending frame flag, and current size.
- [x] On `App::request_redraw`, mark affected native outputs dirty but do not
      immediately submit a second commit if a page flip is pending.
- [x] On `DrmEvent::VBlank`/page-flip completion, clear the pending flag, send
      Wayland frame callbacks for surfaces visible on that output, and schedule
      the next render if dirty.
- [x] Schedule the next render from vblank when the viewport is animating.
- [x] Replace timeout-based animation pacing for UDEV with vblank pacing.
- [x] Use damage from `render_frame` where possible; force full redraw after
      modeset, session activation, connector change, or dmabuf import.
- [x] Flush Wayland clients after frame callbacks.

Done when: native rendering is vblank-paced with no busy loop, animations advance
smoothly, and clients repaint after frame callbacks.

## Step 7: Dmabuf Feedback And Client Buffer Import

Goal: make GPU-accelerated clients work on the native backend.

- [x] Derive the dmabuf feedback main device from the selected render node/DRM
      node rather than the winit EGL display.
- [x] Advertise formats/modifiers compatible with native rendering; client dmabuf
      import is renderer-backed, while GBM scanout compatibility is validated by
      `GbmBufferedSurface` for the compositor's output buffers.
- [x] Import client dmabufs through the native renderer path in
      `process_pending_dmabuf_imports`.
- [x] Force a full redraw after imports, as the winit/headless paths already do.
- [x] Add logs for unsupported formats/modifiers that include enough detail to
      debug GTK/Mesa failures.

Done when: the GTK test app can create EGL buffers and render upright through
Hearthspace on the native backend.

## Step 8: Connector Hotplug And Multi-Output Foundations

Goal: move from one hardcoded output to a connector-backed output set.

- [x] Replace `App::output` and `App::output_size` with an output collection or a
      primary-output abstraction that can grow to multiple outputs.
- [x] On udev `Changed`, re-enumerate connectors for the affected GPU.
- [x] Add newly connected connectors as Smithay `Output`s with globals.
- [x] Disable removed/disconnected outputs and destroy or update their globals in
      the Smithay output manager.
- [x] Recompute pointer/output mapping when output geometry changes.
- [x] Keep multi-monitor layout simple at first: horizontal layout in connector
      enumeration order, or single-primary until a layout policy exists.

Done when: plugging or unplugging a monitor updates Wayland output state without
crashing the compositor.

## Step 9: Multi-GPU And Direct Scanout Later

Goal: defer complexity until the single-GPU path works.

- [x] Keep only one render GPU in the first native backend milestone.
- [x] Add `renderer_multi` only where needed to import buffers from clients or
      outputs on non-render GPUs.
- [ ] Add explicit data structures for render node, scanout node, and per-output
      allocator ownership before enabling secondary GPUs.
- [x] Consider Smithay's DRM compositor helpers/direct-scanout path only after
      normal composited rendering is stable.

Done when: the code structure does not block multi-GPU work, but single-GPU
native rendering remains the only supported path.

## Step 10: Validation Matrix

Goal: keep each backend working while native support lands incrementally.

- [x] `cargo check`
- [x] `cargo check --features udev`
- [x] `cargo check --no-default-features --features udev`
- [x] `cargo test`
- [x] `cargo test --features e2e --test headless_control`
- [ ] Native smoke test on a VT:
      `cargo run --features udev -- --tty --no-shell`
- [ ] Native shell test on a VT:
      `cargo run --features udev -- --tty`
- [ ] VT switch away and back while native Hearthspace is running.
- [ ] Start at least one Wayland client under the native compositor.

## Non-Goals For The First Native Milestone

- [ ] X11/Xwayland support.
- [ ] Direct scanout.
- [ ] VRR/HDR/color management.
- [ ] Complex multi-monitor layout policy.
- [ ] Runtime GPU selection UI.
- [ ] Tablet/touch hardware polish beyond not crashing.
