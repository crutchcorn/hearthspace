# Canvas map

Choose **Canvas map** in Canvas tools to open a popover containing a live,
zoomed-out **Overview** of the canvas. The percentage beside the heading shows
the current zoom. Use the controls below the map to reset zoom to 100%, zoom out,
or zoom in. These controls keep the center of the output that opened the map
in place. Reset does not return you to the canvas origin.

Window outlines show their canvas positions, including windows outside the
current view. An accent-colored outline marks the visible area of the output
whose map was opened; other outputs have muted outlines. The map starts with a wide canvas
neighborhood and expands to include distant windows and viewports, retaining
their relative positions and aspect ratio. It is an orientation aid rather than
a window thumbnail preview; Alt+Tab and inward pinch gestures open the
[window switcher](WINDOW_MANAGEMENT.md#window-switcher) with live previews.

Drag the highlighted viewport area with a mouse or touchscreen to pan the canvas.
Its interior and outline both accept a drag. Click or touch elsewhere in the
map to center your view there, then keep dragging to navigate. The map's
projection stays fixed until release, so the viewport follows the pointer without rescaling
underneath it. Dragging preserves the zoom level and window positions.

Opening the map does not change the viewport or move windows. Zoom controls
keep the popover open. Click the map icon again, click elsewhere on the canvas,
or press Escape to dismiss it. Switching to another shell panel also
closes the map. The map stays in screen coordinates as the canvas zooms.

## Implementation

The Xilem shell draws the map with vector geometry from the existing `status`
command. Geometry refreshes every 100 ms while a map is open; there are no
application screenshots or continuous background map queries when it is closed.
The controls expose the accessible names `Canvas map`, `Reset canvas zoom to 100%`,
`Zoom out`, and `Zoom in`. The header announces `Current canvas zoom`. Reset is
unavailable while the zoom is already 100% or the first map snapshot is pending.
Keyboard users can Tab between the zoom controls and activate them with Enter
or Space; Escape closes the panel.

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
window far away, both zoom directions, reset around the current view, the
percentage label, dragging the highlighted viewport and dismissal without a
subsequent canvas jump. It retains screenshots, status and semantic observations
alongside cleanup evidence.
The optional `--touchscreen` flag repeats the viewport drag through an owned
uinput touchscreen and libinput, checking contact release and device removal.
It requires the existing `libinput-tools` and `python3-evdev` test prerequisites.
