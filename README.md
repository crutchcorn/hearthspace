# Hearthspace

Hearthspace is an experimental Wayland-only Linux desktop environment. The core idea is to manage application windows on an infinite 2D canvas, similar to using a design canvas for windows.

The current implementation is a nested proof-of-concept compositor built with Rust and Smithay. It runs inside an existing Wayland desktop session.

## Run

```sh
cargo run
```

For temporary scroll-zoom testing in environments where the host intercepts `Super` + scroll, run:

```sh
cargo run -- --scroll-zooms
```

In this mode, vertical scroll zooms the canvas without requiring `Super`, so scroll will not be forwarded to application windows.

This opens a nested compositor window and creates its own Wayland socket:

```text
WAYLAND_DISPLAY=wayland-hearthspace-0
```

Spawned child applications receive this `WAYLAND_DISPLAY` and connect back to Hearthspace instead of the host desktop compositor.

## Test The PoC

The top control bar is a GPUI shell client with a spawn dropdown plus global controls:

```text
SPAWN v | LEFT | RIGHT | UP | DOWN | ZOOM+ | ZOOM- | LOG
```

Use them to:

```text
SPAWN > A11yTest: spawn the built-in GTK accessibility test app inside Hearthspace
SPAWN > Foot: spawn a Foot terminal inside Hearthspace
left/right/up/down: pan the canvas by half the compositor window size
ZOOM+/ZOOM-: zoom the canvas in and out around the viewport center
LOG: print AT-SPI accessibility trees for Hearthspace-managed windows to the compositor log
```

Spawned app windows are rendered in canvas coordinates. Panning animates the viewport offset, moving all client windows together relative to the visible compositor window. Zooming animates the viewport scale while keeping the GPUI toolbar fixed in screen-space.

The basic transform is:

```text
screen_position = canvas_position - viewport_offset
```

Window interaction:

```text
Left click app content: focus and interact with the app
Left click title bar: focus and raise the window
Left drag title bar: move the window on the canvas
Drag client-side app header bars: move the window when the app requests xdg_toplevel.move
SPAWN: place the new window near the current viewport center
Super + two-finger scroll up/down: smoothly zoom the canvas in/out
```

Hearthspace advertises no xdg-shell minimize/maximize/fullscreen capabilities.
GTK clients spawned by Hearthspace also use private runtime GTK/GSettings config
with `gtk-decoration-layout=:close` and `button-layout=':close'`, so their
header bars show only a close button without changing the host GNOME session.

Note: this shortcut is expected to work on a native Ubuntu/GNOME session. In the current Parallels VM test environment, `Super` is detected by Hearthspace, but scroll events may not be delivered to the nested compositor until after `Super` is released.

Use `--scroll-zooms` as a temporary testing override in that environment.

## Current Scope

Implemented:

```text
Nested Wayland compositor window
Wayland client socket
xdg-shell client acceptance
GLES rendering path
Dirty/event-driven render loop that skips GPU redraws when the scene is unchanged
Timer-based per-window idle-state daemon with configurable idle levels
Synthetic wl_output and xdg-output advertisement
xdg-decoration advertisement with server-side decoration requests
GPUI shell-client control bar
Compositor-owned draggable title bars only for explicitly server-side-decorated windows
Spawn dropdown for the built-in GTK accessibility test app and Foot
Canvas viewport offset
Half-screen pan targets
Animated pan buttons
Animated zoom buttons
Super-modified touchpad or mouse-wheel zoom
AT-SPI accessibility tree logging from the GPUI shell bar
Stable Hearthspace window IDs in accessibility logs
Window focus, raise, and title-bar dragging
xdg-decoration-aware client-side versus server-side window chrome
Input-region-aware pointer forwarding to client surfaces
```

Still intentionally rough:

```text
No window resizing yet
Zoom supports shell buttons and Super-modified scroll, but there is no pinch gesture zoom yet
AT-SPI logging is scoped by matching Hearthspace-managed window app IDs/titles against AT-SPI application roots and non-shell descendants; this is a heuristic until windows have direct AT-SPI object references
Apps must publish AT-SPI data to appear in semantic logs; the built-in GTK test app exists to provide deterministic semantic content
Several optional desktop protocols are not implemented yet, so clients may print warnings
```

Deferred:

```text
Full login-session desktop environment integration
DRM/KMS backend and libinput device management
X11/Xwayland support is intentionally out of scope unless a concrete need appears
Minimization, task bars, workspaces, panels, or richer launchers
Persistence of window positions
Multi-monitor support
Accessibility integration
Theming beyond the current proof-of-concept shell UI
```
