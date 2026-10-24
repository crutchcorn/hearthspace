# Canvas map

Choose **Canvas map** in Canvas tools to open a popover containing a live,
zoomed-out map of the canvas and icon buttons for zooming out and in. The
percentage between the buttons reflects the current canvas zoom.

Window outlines show their canvas positions, including windows outside the
current view. An accent-colored outline marks the visible area of the output whose map was
opened; other outputs have muted outlines. The map starts with a wide canvas
neighborhood and expands to include distant windows and viewports, retaining
their relative positions and aspect ratio. It is an orientation aid rather than
a window thumbnail preview; Alt+Tab and inward pinch gestures open the
[window switcher](WINDOW_MANAGEMENT.md#window-switcher) with live previews.

Drag the highlighted viewport area with a mouse or touchscreen to pan the canvas.
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

The popover is a client-sized Xilem/Winit xdg toplevel in screen coordinates.
The compositor recognizes its `dev.hearthspace.canvas-map` app ID and anchors
it beside the control according to the current frame layout. Its focus group
allows the toggle to close on release without reopening. Normal application
focus, window switching and lock transitions dismiss it. Smithay continues to
handle application xdg popups; the map requires no new Wayland protocol.

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
scripts/map-smoke --session "$PWD/artifacts/map-native-1" --touchscreen
```

Use a fresh absolute session path and follow [native testing](NATIVE_TESTING.md).
The workflow checks real pointer activation, live minimap pixels after moving a
window far away, both zoom directions, the percentage label, dragging the highlighted
viewport and dismissal without a subsequent canvas jump. It
retains screenshots, status and semantic observations alongside cleanup evidence.
The optional `--touchscreen` flag repeats the viewport drag through an owned
uinput touchscreen and libinput, checking contact release and device removal.
It requires the existing `libinput-tools` and `python3-evdev` test prerequisites.
