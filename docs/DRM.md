# DRM/KMS Backend

This document explains Hearthspace's display backend model: what changes when we
run nested inside another desktop versus when we own the display natively, what
DRM/KMS is responsible for, and which boundaries contributors should preserve.

The policy is:

- Keep compositor behavior backend-neutral wherever possible.
- Use the nested winit backend for everyday development inside an existing
  desktop session.
- Use the native udev/DRM/KMS backend when Hearthspace owns a seat, reads input
  devices directly, and presents frames directly to displays.
- Treat native display ownership as an operating-system integration boundary,
  not just another windowing mode.

## The Mental Model

Hearthspace has one compositor core and multiple display backends.

The compositor core owns:

- Wayland protocol handling.
- Window management, stacking, hit testing, and focus.
- Canvas and viewport state.
- Shell command handling.
- Shared scene construction and rendering logic.

A display backend owns:

- How pixels reach a display target.
- How input events enter the compositor.
- How frame timing is paced.
- How renderer buffers are acquired and submitted.
- Which OS resources need to be opened, paused, resumed, or released.

Keep those concerns separate. A window drag, resize edge, popup, client commit,
or viewport animation should not need to know whether Hearthspace is nested in a
host compositor or running as the native compositor on a VT.

## Backend Modes

Hearthspace supports three display modes:

- `winit`: nested backend for normal development. Hearthspace is a client of an
  existing Wayland/X11 desktop and renders into a host-managed window.
- `headless`: offscreen backend for screenshots, command-socket tests, and e2e
  automation.
- `udev`/DRM/KMS: native backend. Hearthspace owns a session, opens DRM and input
  devices, renders into scanout-capable buffers, and presents directly to KMS.

Backend selection is intentionally explicit when requested:

```text
--headless -> headless
--winit    -> winit
--tty      -> udev/DRM/KMS
auto       -> winit when WAYLAND_DISPLAY or DISPLAY is set, otherwise udev
```

Native smoke and triage commands live in `docs/NATIVE_TESTING.md`.

## Nested vs Native Ownership

The nested backend borrows a display from another compositor. The native backend
owns the display stack directly.

| Concern | Nested winit | Native udev/DRM/KMS |
| --- | --- | --- |
| Display ownership | Host compositor owns outputs | Hearthspace owns KMS outputs |
| Presentation | Submit to a host window | Commit GBM buffers to CRTC/connector targets |
| Input | Host windowing system translates input | libinput reads devices through the active session |
| Frame pacing | Host/window events drive redraws | Vblank/page-flip completion paces redraws |
| Cursor | Host cursor presentation | Hearthspace-owned hardware/software cursor policy |
| Screenshots | Renderer readback | Requires native readback support |

This ownership difference is why native mode has more failure modes. It must
handle session activation, session pause, DRM hotplug, input-device access, mode
selection, scanout buffers, and page-flip completion. Nested mode delegates most
of that to the host desktop.

## Native Display Stack

DRM/KMS is the Linux kernel display API used for native presentation.

The key concepts are:

- DRM device: kernel device node representing a GPU/display device.
- Connector: physical or virtual display connection, such as HDMI, DisplayPort,
  or a VM display connector.
- CRTC: scanout engine that drives a mode on a connector.
- Mode: display resolution and refresh timing.
- Plane: scanout layer. The primary plane carries the main framebuffer; cursor
  planes may carry hardware cursor buffers.
- KMS commit: request to present a framebuffer or update display state.
- Vblank/page flip: completion signal that a submitted frame reached scanout.

GBM and EGL/GLES sit beside DRM/KMS:

- GBM allocates buffers that can be rendered into and scanned out.
- EGL creates the rendering context over the native device.
- GLES renders Hearthspace's scene into the acquired buffer.
- KMS presents that buffer to the selected output.

Keep this split in mind: rendering produces pixels; KMS presents buffers; vblank
tells us when presentation completed.

## Session And Device Access

Native mode must not open input and DRM devices as arbitrary files. It uses a
seat/session manager so device access follows Linux desktop rules.

The session layer is responsible for:

- Granting access to DRM and input device fds.
- Telling Hearthspace which seat is active.
- Pausing device use when the VT/session is no longer active.
- Re-activating device use when the session returns.

When the session pauses, native code should stop committing to DRM and stop
processing direct input. Wayland clients remain connected; the compositor is
paused at the display/input boundary, not logically torn down.

When the session activates again, native code should re-open or reactivate
devices as needed, rescan connectors, recreate KMS surfaces if required, and
force a redraw.

## Output Model

Wayland clients see `wl_output` globals. Native KMS sees connectors, CRTCs, and
modes. Hearthspace bridges those with compositor-owned output records.

The output model should preserve these rules:

- Client-visible output state is compositor state.
- KMS connector state is backend state.
- A KMS connector can update, disappear, or reappear without changing the rest of
  the compositor model.
- Pointer bounds, shell bars, and output layout should be reconciled when output
  geometry changes.

Hearthspace's native path is intentionally staged: render through a selected
primary DRM device and primary output target first, while keeping client-visible
output state capable of representing connected secondary outputs. Future
multi-output rendering should preserve the same boundary: KMS owns scanout
surfaces; compositor output records own client-visible geometry.

## Render And Scanout Ownership

Native rendering has two separate ownership roles, even when one implementation
backs both roles with the same DRM node:

- Render ownership: EGL/GLES renderer, dmabuf imports, texture sampling, and
  scene rendering.
- Scanout ownership: KMS resources, connector/CRTC/mode state, GBM surfaces, and
  page flips.

Keep those roles separate in the code model. That makes future work easier for
render nodes, scanout nodes, multiple GPUs, per-output allocators, and device
handoff.

The normal native frame flow is:

1. Something marks compositor state dirty.
2. The event loop asks the native backend for a scanout buffer.
3. The shared renderer draws the scene for the output's logical render view.
4. The native backend queues the buffer for KMS presentation.
5. Page-flip/vblank completion marks that output ready for another frame.
6. Deferred redraws or animations schedule the next frame after completion.

Do not submit endless native frames just because the event loop is awake. Native
mode should be paced by vblank/page-flip completion so it does not render ahead
of scanout.

## Event Loop And Pacing

All backends share the same calloop-driven compositor loop. Native mode adds OS
event sources for:

- Session pause/activation.
- DRM device hotplug and connector changes.
- Direct input through libinput.
- DRM page-flip/vblank completion.

The event loop should remain the coordination point. It imports pending client
dmabufs, advances compositor state, applies cursor state, renders when dirty,
flushes clients, and cleans transient Wayland state.

In native mode, redraw requests are dirty-state requests, not immediate page
flips. If a KMS frame is already pending, the backend should remember that more
work is dirty and submit the next frame after the current page flip completes.

## Dmabuf Feedback

GPU clients need to know which device and formats they should allocate buffers
for. Hearthspace advertises linux-dmabuf feedback based on the renderer backend.

The rule is: client buffers should be compatible with the renderer that will
sample them.

For native mode, the advertised main device comes from the opened DRM/render
device. The renderer's importable formats become the advertised dmabuf formats.
Client dmabufs are imported into the native renderer before they are composited.

Import failures are recoverable client/rendering compatibility failures, not KMS
mode-setting failures. Logs should include enough detail to debug them: size,
format, plane count, modifier presence, and node.

See `docs/RENDERING.md` for client-buffer readiness and dmabuf synchronization.

## Damage Tracking And KMS Damage

There are two different damage concepts:

- Renderer damage: regions the compositor needs to redraw into its render target.
- KMS damage clips: regions the backend asks the kernel/display stack to treat as
  changed for scanout.

Renderer-side damage tracking is useful and should stay enabled. It reduces GLES
work when the scene only changes partially.

KMS damage clips are a separate optimization and should be treated as optional.
Some virtual or driver stacks reject framebuffer damage clips during commits. If
KMS damage clips are unreliable on a device, the compositor should submit the
frame without KMS damage clips and keep running.

Hearthspace's native policy is conservative: keep renderer-side damage, but do
not forward damage clips to KMS by default. Enabling KMS damage should be per
output/device and should fall back cleanly if a commit fails with an
unsupported-parameter error.

## Failure Model

Native backend errors fall into two broad classes:

- Initialization failures: required resources cannot be created, so native mode
  cannot start correctly.
- Live/recoverable failures: a client import, frame callback, flush, hotplug
  update, or individual render submission fails while the compositor can keep
  running.

Prefer keeping the compositor alive for live recoverable failures. Mark affected
state dirty, force a full redraw when appropriate, log enough context to debug
the device/client, and continue serving Wayland clients.

Session pause is not an error. It is expected OS behavior. During pause, DRM
commits and direct input should stop. During activation, device and output state
should be rebuilt enough to resume presentation.

Useful native safety exits are documented in `docs/NATIVE_TESTING.md` and include
the emergency keyboard chords plus `SIGINT`, `SIGTERM`, and timed exits.

## Contributor Rules

Use these rules when changing native display code:

- Keep compositor behavior backend-neutral unless the behavior is truly about OS
  display/input integration.
- Keep session/device access behind the native backend.
- Do not let input handlers or window-management code submit KMS frames directly.
- Treat redraw requests as dirty state; let backend pacing decide when a frame is
  submitted.
- Keep render ownership and scanout ownership separate even if a backend shares a
  DRM fd between them.
- Keep renderer damage and KMS damage clips conceptually separate.
- Prefer recoverable handling for live native failures.
- Update `docs/NATIVE_TESTING.md` when changing native run commands, expected
  logs, or failure triage.
- See `docs/CURSOR.md` for cursor-specific native presentation policy.
