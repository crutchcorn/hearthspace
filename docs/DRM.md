# DRM/KMS Backend Notes

Evergreen reference for working on Hearthspace's display backends. This explains
how the native (DRM) path differs from the nested (winit) path we develop
against day-to-day, and what a real DRM backend entails.

## Nested (winit) vs. native (DRM)

Hearthspace can run two ways:

- **Nested (winit backend):** Hearthspace is itself a Wayland/X11 client that
  gets a window inside an existing desktop session. This is the default for
  development — it coexists with the host desktop, snapshots, the Xilem shell,
  and the GTK test app.
- **Native (DRM/KMS backend):** Hearthspace runs directly on the GPU and a TTY
  with no host compositor beneath it. This is "real-world" usage.

| Concern        | winit (nested)                       | DRM/KMS (native)                          |
| -------------- | ------------------------------------ | ----------------------------------------- |
| Where it runs  | A window inside another compositor   | Bare metal on a VT/TTY, owns the display  |
| Display output | Host compositor presents the window  | Direct KMS mode-setting on a connector    |
| Frame timing   | Host decides; winit must be pumped   | Kernel delivers vblank/page-flip on an fd |
| Input          | winit translates host input          | libinput reads evdev devices via fds      |
| Loop model     | `pump_events` (event source on winit)| Pure epoll: block until an fd is ready    |

## Why DRM is the "fully event-driven" endgame

On DRM every wakeup source is a real file descriptor that epoll/calloop can wait
on directly:

- **DRM device fd** — fires when a page-flip/vblank completes, so you redraw
  exactly when the hardware is ready for the next frame (vblank-paced).
- **libinput fd** — fires on actual input.
- **Wayland display fd** — fires on client requests.

The loop becomes "block until one of these is readable, handle it, repeat." No
polling, no `sleep`; the kernel wakes you only when there is work.

> Note: the winit backend is *also* event-driven in Smithay 0.7 —
> `WinitEventLoop` implements calloop's `EventSource` (winit's loop is
> epoll-backed). So the 1 ms busy-poll the project started with was an artifact
> of the hand-written loop, **not** a limitation of winit. We do not need DRM to
> remove the busy-poll.

## What a real DRM backend requires

Going native is a substantial lift beyond a feature flag:

1. **Session & seat management** — acquire the seat and DRM-master via
   logind/`libseat` (`backend_session_libseat`). Required to drive a GPU/VT
   without root and to handle VT switching.
2. **Device discovery** — **udev** (`backend_udev`) to enumerate GPUs and input
   devices and to handle hotplug.
3. **Input** — **libinput** (`backend_libinput`) reading evdev devices, fed into
   the same Smithay `Seat`/pointer/keyboard the winit path already uses.
4. **KMS mode-setting** — `backend_drm`: enumerate connectors/CRTCs, pick modes,
   create a `DrmDevice`/`DrmSurface`, and schedule page flips. Handle vblank
   completion events to pace rendering.
5. **Buffer allocation & rendering** — GBM allocator + DMA-BUF, and typically
   `renderer_multi` for multi-GPU import/export. The actual draw code can stay
   shared because both backends expose a `GlesRenderer`.
6. **Output management** — connectors map to Smithay `Output`s (the winit path
   hardcodes a single output today).

## Cargo features

Keep DRM deps behind a feature so everyday builds stay light:

```toml
[features]
default = ["winit"]
winit = ["smithay/backend_winit"]
udev = [
  "smithay/backend_drm",
  "smithay/backend_libinput",
  "smithay/backend_session_libseat",
  "smithay/backend_udev",
  "smithay/renderer_multi",
]
```

- Dev: `cargo run` (winit only, fast).
- Native: `cargo run --features udev -- --tty` (or auto-detected on a bare TTY).

The udev module sits behind `#[cfg(feature = "udev")]`.

## What stays shared between backends

Almost everything. All Wayland protocol handlers, window management
(`compositor/windows.rs`), hit-testing, the idle daemon (`compositor/idle.rs`),
viewport/geometry, the shell + command socket, and the Xilem shell are
backend-neutral. Only output creation, frame scheduling, renderer acquisition,
and the input source differ — these are the parts isolated behind the `Backend`
seam.

## Backend selection (intended behavior)

```text
nested = WAYLAND_DISPLAY is set || DISPLAY is set
--tty  forces DRM/udev ; --winit forces nested
default: nested -> winit, otherwise -> udev
```

## Reference

Smithay's reference compositor **anvil** implements winit, x11, and udev/DRM
backends over one shared state and is the canonical model for this structure.
