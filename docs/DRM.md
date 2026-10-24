# DRM/KMS backend

Hearthspace can run inside another desktop or own a Linux seat and its displays.
The compositor keeps the same window, canvas, and input policies in either mode;
the backend supplies device access, rendering targets, and presentation timing.
This guide is for contributors changing that boundary.

## Backend modes

| Mode | Display and input ownership | Typical use |
| --- | --- | --- |
| `winit` | A host Wayland/X11 desktop supplies a window and input events | Nested development and interaction checks |
| `headless` | An offscreen renderer supplies the output | Deterministic integration tests and capture |
| `udev` / DRM/KMS | Hearthspace acquires seat devices and presents to KMS | Native desktop sessions |

Explicit compositor flags select the backend:

```text
--headless -> headless
--winit    -> winit
--tty      -> udev/DRM/KMS
auto       -> winit when WAYLAND_DISPLAY or DISPLAY is set, otherwise udev
```

The selected backend must be enabled in the build. Nested and native support use
`winit` and `udev` features respectively. See [development](DEVELOPMENT.md) for
build and run commands, and [native testing](NATIVE_TESTING.md) for VT ownership,
recovery, and acceptance procedures.

## Compositor and backend ownership

The compositor core owns Wayland protocols, window identity and stacking, hit
testing, focus, the canvas viewport, shell requests, and scene construction.
A drag or resize should use those shared policies regardless of the display
backend.

The backend owns how input enters the event loop, how render buffers are
acquired, how they reach a display, and which OS resources must pause or resume.
Native display ownership includes hotplug, modes, page flips, and device access;
it is more than replacing a nested window's framebuffer.

| Concern | Nested winit | Native DRM/KMS |
| --- | --- | --- |
| Outputs | Host manages physical displays | Hearthspace manages connector/CRTC targets |
| Rendering target | Host window surface | GBM buffers suitable for scanout |
| Input | Host-translated events | libinput through the active seat session |
| Presentation pacing | Host/window events and buffer submission | Pending frames and DRM completion events |
| Named cursor | Host window cursor | Hardware cursor with software fallback |
| Capture | Renderer readback | Output readback associated with presentation, with cursor overlay when needed |

## Native display concepts

DRM/KMS is the kernel API for configuring displays and presenting buffers.
A connector represents a display connection, such as HDMI or DisplayPort. A CRTC
is a scanout engine driving a mode on a connector. A mode describes resolution
and refresh timing. Planes carry scanout layers; the primary plane holds the main
framebuffer and a cursor plane can carry a separate pointer image.

GBM allocates renderable scanout buffers. EGL creates the graphics context, GLES
renders the scene, and a KMS commit submits the buffer for presentation. A
page-flip completion is evidence about the submitted frame reaching scanout;
finishing GLES scene construction is not.

Render ownership and scanout ownership remain separate roles even when they use
the same DRM node. The renderer owns imports and texture resources; scanout owns
connectors, CRTCs, output surfaces, and pending flips. Preserve that distinction
when extending device support.

## Outputs and scaling

The native backend selects one primary DRM device and renders its connected
output targets. Each target has its own GBM surface, damage tracker, and pending
frame state. The compositor publishes the corresponding Wayland outputs and
renders the desktop rectangle belonging to each one.

Multiple outputs on that device are supported. Additional DRM devices are
discovered but are not independent rendering targets; cross-device rendering and
GPU handoff remain limitations. Do not describe a second GPU's connected outputs
as available merely because device discovery found it.

Output records own client-visible mode, scale, and logical placement. KMS targets
own physical scanout dimensions. The output view is logical, and rendering applies
its integer scale to reach the framebuffer. Pointer bounds, per-output shell
surfaces, lock surfaces, and captures must be reconciled when geometry changes.
See [size units](SIZE_UNITS.md).

Saved preferences can outlive a disconnected connector. Live outputs and saved
monitor rows therefore have separate lifetimes; hotplug must not erase user
preferences or manufacture a currently connected output from a saved row.

## Session and device access

Native mode uses Smithay's libseat session integration for DRM and input-device
access. Device use follows the active seat session. It must not bypass that
ownership by treating device nodes as ordinary files available at all times.

On session pause, Hearthspace stops native commits and direct input, resets active
input sequences, and invalidates capture and lock-presentation evidence. Wayland
clients remain connected. An inactive VT is an expected session state, not a
request to retire every application window.

Activation rebuilds or reactivates devices and output surfaces, reconciles output
state, and requests a redraw. System-sleep recovery also has an explicit path:
a wake signal alone is not proof that libseat paused and resumed the devices.
Recovery uses bounded retries and requires presentation on the expected outputs
before declaring success. Driver calls are synchronous, so a retry budget cannot
guarantee a hard upper bound on a blocked driver call.

Locking retains an opaque fallback while outputs are unavailable or being rebuilt.
A fresh lock presentation must satisfy the current output barrier; a stale page
flip cannot acknowledge it. See [session locking](SESSION_LOCK.md).

## Event loop and pacing

All backends share the calloop-driven compositor loop. Native mode adds session,
udev, libinput, and DRM event sources. The loop imports pending dmabufs, advances
state, applies cursor policy, renders dirty work, and dispatches client events.

For each native output:

1. A state change marks work dirty.
2. If no frame is pending, the backend acquires a scanout buffer.
3. The shared renderer draws that output's logical view at its output scale.
4. The backend queues the buffer and records a pending frame.
5. A matching completion releases the pending frame and allows further work.

Dirty work arriving during a pending flip is retained. Input and window-management
handlers request redraws; they do not submit KMS frames themselves. Completion
handling rejects events from retired surfaces, and an untimed modeset event can
unblock the buffer queue without proving capture or lock presentation. Those
paths request a fresh, verifiable frame.

## Dmabuf feedback

GPU clients need device and format information for compatible allocation.
Hearthspace advertises renderer-importable dmabuf formats and uses the native
renderer device as the feedback main device. Imported buffers are then sampled
by that renderer.

Import failures are client/rendering compatibility failures, distinct from KMS
mode-setting failures. Useful diagnostics include dimensions, format, planes,
modifiers, and the selected device. An advertised format is not a promise that
every allocation from every driver will import. See
[rendering](RENDERING.md#client-buffers-and-readiness) for readiness and synchronization.

## Damage tracking and KMS damage

Renderer damage describes regions that must be redrawn in a framebuffer. KMS
damage clips describe changed regions passed to the kernel for presentation.
They serve different consumers and can be enabled independently.

Hearthspace retains renderer-side damage tracking but submits native frames
without KMS damage clips. This avoids relying on optional driver behavior for
correct presentation. A future KMS-damage optimization needs device-appropriate
validation and a fallback when the driver rejects it; it must not disable ordinary
renderer damage as a side effect.

## Failure and diagnostics

Initialization can fail when required seat, graphics, or output resources cannot
be created. During a running session, client import failures, hotplug changes, or
individual submissions can be recoverable. Keep clients connected where possible,
retain dirty work, and log the affected device or output. Remaining alive does not
by itself prove that presentation recovered.

Native captures use renderer readback and presentation acknowledgement. Lock,
output reconfiguration, device removal, and inactive-session transitions invalidate
pending capture state. A screenshot should not be reported as a newly presented
frame on the basis of stale pixels alone.

Use [native testing](NATIVE_TESTING.md) for safety exits and failure triage,
[cursor handling](CURSOR.md) for native cursor fallback, and
[tested environments](TESTED_ENVIRONMENTS.md) for recorded environments. A nested
or headless pass does not establish native device compatibility.
