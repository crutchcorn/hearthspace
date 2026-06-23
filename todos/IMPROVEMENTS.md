# Hearthspace Technical Improvements

A running list of improvements identified during a technical review of `src`.

## Architecture / Runtime Core

- **✅ Done — Replace the polling event loop with an event-driven one.**
  `run_winit` in [src/compositor/mod.rs](src/compositor/mod.rs) now runs on a
  `calloop` event loop driven by epoll: it blocks when idle and only wakes per
  frame (16 ms) while a viewport animation is in flight. The old
  `std::thread::sleep(IDLE_SLEEP)` busy-loop is gone.
- **✅ Done — Implement real damage tracking.** Rendering now goes through an
  `OutputDamageTracker` (`render_frame` in
  [src/compositor/rendering.rs](src/compositor/rendering.rs)) that compares each
  frame's elements against the previous frame and only clears/redraws the
  changed regions, using the winit back buffer age. The damaged region is fed
  back into `submit`, and a frame with no damage is skipped entirely. Window
  surfaces are built at native scale and wrapped in a `RescaleRenderElement` so
  the viewport zoom is applied in a single output coordinate space.

  The title-bar/close-button `SolidColorBuffer`s are persisted per window
  (`WindowDecorationBuffers` on `ManagedWindow`) and only `update`d when their
  size or color changes, so their render-element ids stay stable and the tracker
  skips them while the title bar is unchanged.
- **✅ Done — Avoid blocking the main loop on the command socket.**
  Each accepted connection is now registered as its own non-blocking `calloop`
  source (`accept_command_connections` in
  [src/compositor/shell_integration.rs](src/compositor/shell_integration.rs)).
  Data is read incrementally in 1 KiB chunks and parsed line-by-line, so a slow
  or stuck client can no longer stall the compositor. A 4 KiB per-connection
  buffer cap guards against a client streaming data without a newline.

## Correctness / Robustness

- **Stop using `Vec` indices as window identity.** `windows: Vec<ManagedWindow>`
  doubles as z-order and identity, and `window_index` is passed around (e.g.
  `DragState.window_index`). Vector mutation (a window closing mid-interaction)
  can invalidate a stored index. Stable `window.id` values already exist; use
  them for lookups to remove a class of latent bugs.
- **Tighten `request_mode` mode handling.** The wildcard arm silently coerces
  unknown decoration modes to server-side.
- **Add tests for compositor window-management / hit-testing logic.** Currently
  untested (understandable given Smithay's types), but it's where bugs will hide.

## Performance

- **Cache per-commit bounding boxes.** `title_bar_canvas_rect` calls
  `bbox_from_surface_tree` (a surface-tree walk) multiple times per window per
  frame (rendering, close-button rects, hit-testing). `hit_test` runs full
  surface-tree traversals up to three times per button event in
  [src/compositor/input.rs](src/compositor/input.rs). Cache the bbox per commit.

## Observability

- **Make logging consistent.** `tracing-subscriber` is initialized in
  [src/main.rs](src/main.rs) but never used — the codebase logs via
  `println!`/`eprintln!` everywhere (including `log_idle_transition` and spawn
  error paths). Either adopt `tracing` macros or drop the dependency.

## Minor / Cleanup

- **Collapse redundant match arms.** `decoration_for_new_window` in
  [src/compositor/windows.rs](src/compositor/windows.rs) has two arms returning
  the same value.
- **Replace remaining magic numbers** with named constants (e.g. `- 180` in
  `prepare_spawn_position`, the `x: 80, y: 96` spawn seed).
