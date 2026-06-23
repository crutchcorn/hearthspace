# Plan

## Goal

Build a Wayland-only Linux desktop environment proof-of-concept where application windows live on an infinite 2D canvas. The first milestone should prove the core model: spawn windows, pan the viewport, and render windows at canvas-relative positions.

## Technical Direction

Use Rust and Smithay for the compositor.

Start with Smithay's nested `winit` backend instead of a full DRM/KMS/session compositor. Nested mode lets us run the compositor manually inside an existing Wayland desktop session while we prove the window/canvas model.

Use GLES rendering via Smithay's renderer path. Avoid a full GUI toolkit for the first control surface; render a small compositor-owned control bar directly.

## Initial Runtime Model

The compositor owns a Wayland display socket, for example `wayland-hearthspace-0`.

Launching the compositor manually should eventually look like:

```sh
cargo run
```

The compositor sets `WAYLAND_DISPLAY` for child processes it spawns. Spawned apps connect back to this compositor, not to the host desktop compositor.

## Canvas Model

Store window positions in canvas/world coordinates.

```text
screen_position = canvas_position - viewport_offset
```

The viewport offset represents the part of the infinite canvas currently visible on screen.

For the first proof-of-concept, use a fixed spawn position strategy such as placing each new window slightly offset from the previous one in canvas coordinates.

## Required First Controls

Render a simple compositor-owned control bar with five buttons:

```text
Spawn App | Left | Right | Up | Down
```

Button behavior:

```text
Spawn App: launch a configured Wayland app, defaulting to foot
Left: viewport.x -= output_width / 2
Right: viewport.x += output_width / 2
Up: viewport.y -= output_height / 2
Down: viewport.y += output_height / 2
```

The control bar can initially be drawn above app surfaces and consume pointer clicks in its rectangle.

## Milestone 1: Project Skeleton

Create a Rust workspace with one binary crate for the compositor.

Dependencies should start from the minimal Smithay nested example feature set:

```text
smithay = { version = "0.7.0", default-features = false, features = ["backend_winit", "renderer_gl", "wayland_frontend"] }
```

Add only the direct dependencies needed to support the nested event loop, logging, and spawning child processes.

## Milestone 2: Nested Compositor Window

Bring up a nested compositor window using Smithay's `winit` backend.

Requirements:

```text
The compositor opens a host desktop window
The compositor creates a Wayland listening socket
The compositor accepts Wayland clients
The compositor renders a solid background
The compositor exits cleanly when the host window closes
```

## Milestone 3: App Spawning

Implement a `spawn_app` action that launches `foot` with the compositor's Wayland display in its environment.

Requirements:

```text
Clicking Spawn App launches one new client
The launched client connects to the nested compositor
The client creates an xdg toplevel surface
The surface is tracked in compositor state
```

## Milestone 4: Window Rendering On Canvas

Track each xdg toplevel with a canvas position.

Render each surface tree at:

```text
canvas_position - viewport_offset
```

Requirements:

```text
Multiple app windows are visible at distinct positions
Panning changes all app screen positions consistently
Frame callbacks are sent so clients continue drawing
```

## Milestone 5: Control Bar Input

Handle pointer input enough to click the compositor-owned buttons.

Requirements:

```text
Pointer clicks on Spawn App launch a new app
Pointer clicks on pan buttons update viewport_offset
Control bar clicks are not forwarded to client windows
```

## Milestone 6: Basic Client Input

Route keyboard and pointer input to the focused client surface.

Requirements:

```text
Clicking a window focuses it
Keyboard input is forwarded to the focused window
Pointer motion and button events are forwarded to the window under the cursor
Coordinates are transformed from screen space into surface-local space
```

## Deferred Work

These are intentionally out of scope for the first proof-of-concept:

```text
Full login-session desktop environment integration
DRM/KMS backend
libinput device management
X11 or Xwayland support
Window dragging and resizing
Zooming the canvas
Minimization, task bars, workspaces, panels, or launchers
Persistence of window positions
Animations
Multi-monitor support
Accessibility integration
Theming or a full GUI toolkit integration
```

## Validation So Far

The development VM has the required Rust toolchain and native compositor development libraries installed. The Smithay 0.7.0 `minimal` nested compositor example check-builds successfully with:

```text
backend_winit
renderer_gl
wayland_frontend
```

That makes the nested Smithay route viable for the first implementation pass.
