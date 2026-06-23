# Backends: Incremental Path

Plan for supporting both a nested **winit** backend (development) and a native
**DRM/KMS** backend (real-world usage) over one shared compositor state. See
[../docs/DRM.md](../docs/DRM.md) for background on how the backends differ.

The order below keeps every step shippable and shapes the code so the DRM
backend drops in later without reworking shared state.

## Step 1 — calloop event loop + `Backend` seam (winit only) ✅ done

**Goal:** kill the 1 ms busy-poll and make client I/O event-driven, while
introducing the structure DRM will plug into.

- [x] Replace the hand-written loop in `compositor/mod.rs` with a
      `smithay::reexports::calloop` `EventLoop`.
- [x] Introduce shared loop data:
      ```rust
      struct CalloopData { state: App, display: Display<App>, backend: Backend, .. }
      ```
- [x] Insert event sources:
  - [x] Wayland clients: `ListeningSocketSource::with_name(WAYLAND_DISPLAY_NAME)`
        and `insert_client` in its callback.
  - [x] Wayland dispatch: a `Generic` source over
        `display.backend().poll_fd()` that calls `dispatch_clients` +
        `flush_clients`.
  - [x] Command socket: a `Generic` source over the `UnixListener` replacing the
        non-blocking accept loop in `compositor/shell_integration.rs`.
        (The main-loop-blocking `read_to_string` hardening remains a separate
        `IMPROVEMENTS.md` item.)
  - [x] Winit: insert the `WinitEventLoop` directly (it implements calloop
        `EventSource` in Smithay 0.7 — no timer needed).
- [x] Introduce the backend seam so winit-specific bits are isolated:
      ```rust
      enum Backend { Winit(WinitGraphicsBackend<GlesRenderer>) /*, Udev later */ }
      ```
- [x] Drive redraw from `App::needs_redraw` after each dispatch instead of the
      per-iteration `sleep`; block in epoll when idle and wake every
      `ANIMATION_FRAME_INTERVAL` while animating.

**Done when:** `cargo run` behaves identically but the process is idle (no CPU
spin) when nothing changes, and client/command I/O is epoll-driven.

## Step 2 — make rendering renderer-generic ✅ done

**Goal:** decouple the draw path from the winit backend.

- [x] Extract `App::render_frame(renderer: &mut GlesRenderer, framebuffer, output_size)`
      from the winit-specific bind/submit code, now in `compositor/rendering.rs`.
- [x] Both backends call the same `render_frame`; only bind/submit and damage
      handling stay backend-specific.

**Done when:** the winit backend calls `render_frame` and nothing in
`render_frame` references winit types.

## Step 3 — add the DRM/udev backend (feature-gated)

**Goal:** real-world native session. Separate milestone; larger lift.

- [ ] Add cargo features (`winit` default, `udev` optional) per
      [../docs/DRM.md](../docs/DRM.md).
- [ ] `#[cfg(feature = "udev")]` udev module: session/seat via `libseat`, device
      discovery via udev, input via libinput.
- [ ] KMS: enumerate connectors/CRTCs, mode-set, `DrmDevice`/`DrmSurface`,
      page-flip scheduling, vblank-paced redraw.
- [ ] GBM allocator + DMA-BUF; `renderer_multi` for multi-GPU.
- [ ] Map connectors to Smithay `Output`s (replace the hardcoded single output).
- [ ] Backend selection at startup:
      ```text
      nested = WAYLAND_DISPLAY || DISPLAY ; --tty forces udev ; --winit forces nested
      ```

**Done when:** `cargo run --features udev -- --tty` runs Hearthspace natively on
a VT with working input and output, sharing all state with the winit path.

## Notes

- All Wayland handlers, window management, idle daemon, geometry, shell, and the
  GPUI bar are already backend-neutral and need no changes.
- Step 1 is the event-loop fix from `IMPROVEMENTS.md`; Steps 2–3 are the
  multi-backend payoff.
