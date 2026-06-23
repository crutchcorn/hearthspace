# Hearthspace Technical Improvements

A running list of improvements identified during a technical review of `src`.

## Correctness / Robustness

- **✅ Done — Stop persisting `Vec` indices as window identity.**
  `DragState` previously stored a `window_index` that survived across pointer
  events, so a window closing mid-drag could move the wrong window or panic on an
  out-of-bounds index. It now stores the stable `window_id`, and the drag-motion
  handler resolves it each event via `window_mut_by_id`, gracefully no-opping if
  the window is gone. The remaining index usages (`HitTarget`, render/hit-test
  loops) are derived and consumed within a single event without mutating
  `windows`, so they stay safe — `raise_window` already re-derives the index
  after it reorders the vector.
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
