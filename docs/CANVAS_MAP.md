# Canvas map

The map icon beside the notification bell in the shell bar opens a popover containing a
live, zoomed-out map of the canvas and icon buttons for zooming out and in. The
percentage between the buttons reflects the current canvas zoom.

Window outlines show their canvas positions, including windows outside the
current view. A blue outline marks the visible area of the output whose map was
opened; other outputs have muted outlines. The map starts with a wide canvas
neighborhood and expands to include distant windows and viewports, retaining
their relative positions and aspect ratio. It is an orientation aid rather than
a window thumbnail preview; Super+O and overview gestures open live previews.

Drag the orange viewport area with a mouse or touchscreen to pan the canvas.
Its interior and outline both accept a drag. The map's projection stays fixed
until release, so the viewport follows the pointer without the map rescaling
underneath it. Dragging preserves the zoom level and window positions.

Opening the map does not change the viewport or move windows. Zoom controls
keep the popover open. Click the map icon again, the close icon, elsewhere on the
canvas, or press Escape to dismiss it. Switching to another shell panel also
closes the map. The map stays in screen coordinates as the canvas zooms.

## Implementation

The Xilem shell draws the map with vector geometry from the existing `status`
command. Geometry refreshes every 100 ms while a map is open; there are no
application screenshots or continuous background map queries when it is closed.
The icon buttons expose the accessible names `Canvas map`, `Zoom out`, `Zoom in`,
and `Close canvas map`.

The current Xilem/Winit window API creates xdg toplevels, so the popover follows
the existing shell palette model. Its `dev.hearthspace.canvas-map` app ID identifies
a client-sized, screen-space surface below the right edge of its output. It
shares a focus group with that output's bar to allow the map toggle to close on
release without reopening. Normal application focus, overview and lock transitions
dismiss it using ordinary xdg close events. This does not add a Wayland protocol
or replace Smithay's handling of application xdg popups.

For automation, `status.canvas_maps` reports visible popovers with their output
name and logical `x`, `y`, `width`, and `height`. These bounds let agents translate
AccessKit's surface-local physical coordinates correctly on scaled outputs.
`windows` continues to contain only normal applications. The `pan-to x y` control
command sets the canvas offset directly for continuous map dragging, subject to
the same lock and modal-navigation rules as the other canvas controls.

## Regression workflow

Build the native and E2E binaries, then run the shared headless acceptance test:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
HEARTHSPACE_E2E=1 python3 -m unittest discover -s scripts/tests -p test_canvas_map_headless.py -v
```

The same interaction assertions run on real hardware through the owned native
harness, with an isolated accessibility bus and automatic VT restoration:

```sh
scripts/map-smoke --session /tmp/hearthspace-map-native-1 --touchscreen
```

Use a fresh absolute session path and follow [native testing](NATIVE_TESTING.md).
The workflow checks real pointer activation, live minimap pixels after moving a
window far away, both zoom directions, the percentage label, dragging the blue
viewport and dismissal without a subsequent canvas jump. It
retains screenshots, status and semantic observations alongside cleanup evidence.
The optional `--touchscreen` flag repeats the viewport drag through an owned
uinput touchscreen and libinput, checking contact release and device removal.
It requires the existing `libinput-tools` and `python3-evdev` test prerequisites.

## Acceptance evidence

On 2026-10-15, the shared headless acceptance passed in 12.263 seconds with
artifacts in `/tmp/hearthspace-map-headless-2` and its test log at
`/tmp/hearthspace-map-headless-2-test.log`. It verified real pointer activation,
live minimap pixels after moving a window offscreen, both zoom directions and
their percentage labels, and dismissal by Close, Escape, the map toggle and an
outside client click. Dragging the observed blue frame by 12 × 9 logical pixels
panned the canvas by 461 × 346 units while preserving zoom and window geometry;
release and dismissal retained that position. The owned compositor, shell and
private accessibility bus processes were cleaned up.

The first attempt remains in `/tmp/hearthspace-map-headless-1`: its isolated bus
blocked dconf activation, leaving the accessibility tree empty. It was interrupted
and cleaned up. The fixture now uses memory-backed GSettings and launches the
same `hearthspace-shell` binary as the native session. This headless result covers
the pointer path.

Native acceptance also passed on 2026-10-15 in
`/tmp/hearthspace-map-native-1`, on the physical GPD laptop with AMD Radeon
880M/890M (`amdgpu`), kernel `7.0.0-38-generic`, and eDP-1 at 2560 × 1600,
scale 2. The shared assertions passed, followed by pointer and touchscreen drags.
The pointer moved the canvas by 461 × 346 units; the touchscreen moved it by
467 × 351 units. Both preserved zoom and window geometry, stopped after release,
and retained the viewport after dismissal. Screenshots include native page-flip
metadata; the touchscreen trace records DOWN, MOTION, UP and FRAME events from
the temporary uinput device through libinput. This checks the native input path
on one physical output, not manual finger feel or multiple physical monitors.

The owned touchscreen, session, app scopes and runtime were removed, and the
harness restored its recorded original VT, tty2. Final workspace checks passed:
543 Rust tests with E2E/accessibility enabled, 284 Python tests with E2E enabled,
rustfmt, Clippy with warnings denied, Ruff, and the native/test-target build.
The optimized session release `575e4735d06a` is installed; its compositor, shell
and session hashes match `target/release`. Existing desktops keep running their
current binaries until the next session restart.
