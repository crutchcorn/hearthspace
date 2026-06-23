# Hearthspace Technical Improvements

A running list of improvements identified during a technical review of `src`.

## Architecture / Runtime Core

- **Replace the polling event loop with an event-driven one.** `run_winit` in
  [src/compositor/mod.rs](src/compositor/mod.rs) busy-loops and falls back to
  `std::thread::sleep(IDLE_SLEEP)` (1 ms) when idle. Smithay is designed to run
  on a `calloop` event loop driven by epoll; the current poll wastes CPU/power
  and adds latency. This is the highest-impact change.
- **Implement real damage tracking.** Despite threading `damage` rectangles
  through the renderer, every frame redraws the full output via
  `Rectangle::from_size(state.output_size)`. The damage plumbing is currently
  cosmetic.
- **Avoid blocking the main loop on the command socket.**
  `process_shell_commands` in
  [src/compositor/shell_integration.rs](src/compositor/shell_integration.rs)
  accepts on a non-blocking listener but then calls `read_to_string`
  synchronously. A slow or stuck client can stall the whole compositor (local
  DoS). Read with a timeout or move it off the main thread.

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
