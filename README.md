# Hearthspace

Hearthspace is an experimental Wayland-only Linux desktop environment. The core idea is to manage application windows on an infinite 2D canvas, similar to using a design canvas for windows.

The current implementation is a nested proof-of-concept compositor built with Rust and Smithay. It runs inside an existing Wayland desktop session.

## Run

```sh
cargo run
```

This opens a nested compositor window and creates its own Wayland socket:

```text
WAYLAND_DISPLAY=wayland-hearthspace-0
```

## Test The PoC

The top control bar has seven labeled buttons:

```text
SPAWN | LEFT | RIGHT | UP | DOWN | ZOOM+ | ZOOM-
```

Use them to:

```text
SPAWN: spawn a foot terminal inside Hearthspace
left/right/up/down: pan the canvas by half the compositor window size
ZOOM+/ZOOM-: zoom the canvas in and out around the viewport center
```

Spawned app windows are rendered in canvas coordinates. Panning changes the viewport offset, moving all client windows together relative to the visible compositor window. Zooming changes the viewport scale while keeping the toolbar fixed.

Window interaction:

```text
Left click app content: focus and interact with the app
Left click title bar: focus and raise the window
Left drag title bar: move the window on the canvas
SPAWN: place the new window near the current viewport center
```

## Current Scope

Implemented:

```text
Nested Wayland compositor window
Wayland client socket
xdg-shell client acceptance
GLES rendering path
Dirty/event-driven render loop that skips GPU redraws when the scene is unchanged
Synthetic wl_output and xdg-output advertisement
xdg-decoration advertisement with server-side decoration requests
Compositor-owned control bar
Compositor-owned draggable title bars
Spawn button for foot
Canvas viewport offset
Half-screen pan buttons
Manual zoom buttons
Window focus, raise, and title-bar dragging
Input-region-aware pointer forwarding to client surfaces
```

Still intentionally rough:

```text
Button labels use a tiny compositor-drawn block font instead of real text rendering
Closed windows are not cleaned out of the simple position list yet
No window resizing yet
Zoom is button-driven only; there is no wheel or gesture zoom yet
No DRM/KMS full desktop session yet
Several optional desktop protocols are not implemented yet, so clients may print warnings
```
