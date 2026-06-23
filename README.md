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

The top control bar has five icon buttons:

```text
+ | left | right | up | down
```

Use them to:

```text
+: spawn a foot terminal inside Hearthspace
left/right/up/down: pan the canvas by half the compositor window size
```

Spawned app windows are rendered in canvas coordinates. Panning changes the viewport offset, moving all client windows together relative to the visible compositor window.

## Current Scope

Implemented:

```text
Nested Wayland compositor window
Wayland client socket
xdg-shell client acceptance
GLES rendering path
Compositor-owned control bar
Spawn button for foot
Canvas viewport offset
Half-screen pan buttons
Basic keyboard/pointer forwarding to client surfaces
```

Still intentionally rough:

```text
Buttons use simple icons instead of text rendering
Closed windows are not cleaned out of the simple position list yet
Surface hit testing is minimal
No window dragging/resizing yet
No DRM/KMS full desktop session yet
```
