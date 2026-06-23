# Automated Testing Report

A survey of the current state of automated testing in Hearthspace and where it
needs to grow. The goal is to identify the highest-ROI places to add tests and
the infrastructure work required to make harder areas testable.

## Summary

- **20 tests** total, all `#[test]` functions in **4 inline `#[cfg(test)]` modules**.
- Tests are concentrated in the most pure modules: `geometry`, `idle` (core),
  `app_catalog`, and `command`.
- The entire compositor runtime, rendering, input, viewport, windows, shell I/O,
  accessibility, and UI layers have **zero** tests.
- **No test infrastructure**: no `[dev-dependencies]`, no `tests/` integration
  dir, no `benches/`, no doctests, no CI.

## Current Coverage

| File | Tests | What is covered |
|---|---|---|
| [src/geometry.rs](../src/geometry.rs) | 6 | `rect_contains` boundaries, canvas↔screen round-trip, zoom-around-point, zoom clamping, `ease_out_cubic`, interpolation clamping |
| [src/compositor/idle.rs](../src/compositor/idle.rs) | 8 | `IdleTrackerCore` state machine: scheduling, threshold cascades, per-window independence, activity reset, stale-generation rejection, deadline firing order |
| [src/shell/app_catalog.rs](../src/shell/app_catalog.rs) | 4 | Hidden/NoDisplay filtering, `OnlyShowIn` gating, token search match, Exec field-code expansion |
| [src/shell/command.rs](../src/shell/command.rs) | 2 | Wire-name round-trip, `launch-app` id parsing |

## Per-Module Assessment

| Module | Coverage | Testability | Priority | Suggested tests |
|---|---|---|---|---|
| `geometry.rs` | Good | Pure | Low (maintain) | Property tests for round-trip / clamp invariants |
| `shell/command.rs` | Partial | Pure | Med | Negative-parse cases, whitespace, unknown verbs, `label`/`Display` |
| `shell/app_catalog.rs` | Partial | Mostly pure | **High** | Parsing internals (see below) + tempdir-based dir loading |
| `compositor/idle.rs` | Good (core) | Pure core, threaded shell untested | Med | Deadline compaction, `Ord` impl, daemon channel integration |
| `compositor/windows.rs` | None | Rect math pure, hit-test entangled | **High** | Title-bar / close-button / content rect math, z-order insert |
| `compositor/rendering.rs` | None | GPU-bound, one pure helper | Med | `close_button_x_rects` layout |
| `compositor/viewport.rs` | None | Entangled with `App` + `Instant::now` | Med | Extract animation progress to pure fn, then test easing/snapping |
| `compositor/input.rs` | None | Needs Smithay seat | Low–Med | Extract scroll/keysym helpers and test those |
| `compositor/shell_integration.rs` | None | Mostly FS/process I/O | Med | Command buffer framing, path sanitizing, snap-name validation |
| `compositor/mod.rs` | None | Very hard (protocol + winit + GL) | Low | Headless integration harness only |
| `accessibility.rs` | None | Async D-Bus, helpers pure | Med | `accessible_matches_*`, `is_desktop_shell_root`, `indent` |
| `shell/bar.rs` | None | GPUI runtime | Low | Extract result/selection logic; test if decoupled |
| `test_apps/gtk.rs` | None | GTK runtime | Low | Fixture app — not worth testing |
| `config.rs` / `main.rs` / `mod.rs` | None | Trivial / arg dispatch | Low | Optional arg-dispatch test if `main` is extracted |

## Highest-ROI Targets (pure, no mocks needed)

These are pure or near-pure functions with real bug surface and zero coverage
today:

1. **`app_catalog.rs` parsing internals** — `split_exec`,
   `unescape_desktop_value`, `expand_exec_field_codes`, `parse_semicolon_list`,
   `desktop_entry_id`, and `token_score` ranking. String algorithms with many
   edge cases (quotes, escapes, dangling `%`, unterminated quotes).
2. **`rendering.rs::close_button_x_rects`** — pure rectangle layout for the
   close "X"; no GPU dependency despite living in a GPU module.
3. **`windows.rs` rect geometry** — `close_button_canvas_rect`,
   `title_bar_canvas_rect`, `content_canvas_origin`, plus `normal_insert_index`
   / `raise_window` z-order logic. Pure arithmetic over `window.position`.
4. **`shell_integration.rs` command buffering** — newline framing,
   trailing-line-on-close, UTF-8 rejection, `MAX_COMMAND_BUFFER_BYTES`, plus
   `sanitized_path_component` and snap-instance-name validation.
5. **`accessibility.rs` matching helpers** — `accessible_matches_term`
   (case-insensitive substring, trim, empty→false), `accessible_matches_window`,
   `is_desktop_shell_root`. Fully pure.
6. **`viewport.rs` animation math** — extract progress→(offset, scale) into a
   free function and test easing, clamping, and endpoint snapping at
   `progress >= 1.0`.

Several of these are currently private and embedded inside `impl App` or
GPU/protocol modules. Adding tests will require small extract-to-free-function
refactors first.

## Hard-to-Test Areas (need abstraction or integration tests)

- **`compositor/mod.rs`** — `run_winit` owns a calloop event loop, winit GL
  backend, Wayland display, sockets, and a spawned subprocess. The Smithay
  handler impls mutate `App` in response to live protocol objects. Needs a
  headless Wayland client harness.
- **`rendering.rs::render_frame`** — requires a real `GlesRenderer` +
  framebuffer (GPU/EGL). Only the geometry helper is unit-testable.
- **`input.rs::handle_input_event`** — depends on Smithay pointer/keyboard
  handles and `WinitInput` events; needs a seat trait abstraction or
  event-replay integration test.
- **`bar.rs` / `test_apps/gtk.rs`** — require GPUI / GTK runtimes; suited to
  manual or end-to-end UI testing.
- **`accessibility.rs` tree walk** — requires a live AT-SPI D-Bus registry;
  needs a mock connection trait or an integration test against a known peer.
- **`shell_integration.rs` spawning / snap symlinks / GTK config** — touch the
  real filesystem and `Command::spawn`; testable only with tempdir sandboxing
  (`XDG_*` overrides) and dependency injection for the spawn step.
- **`idle.rs::WindowIdleDaemon`** — the core is tested, but the thread + `mpsc`
  + real `Instant`/`recv_timeout` wrapper is not; needs a clock abstraction or a
  timing-tolerant integration test.

## Infrastructure Gaps

- **No `[dev-dependencies]`** in [Cargo.toml](../Cargo.toml). No `proptest`,
  `rstest`, `mockall`, `insta`, `tempfile`, `serial_test`, or async test
  harness. All tests use only `std` + `assert*!`.
- **No `[features]`** to gate hardware/headless code paths for testing.
- **No `tests/` integration directory** and **no `benches/`**, despite
  performance-sensitive paths (damage tracking, render element collection, idle
  deadline compaction).
- **No CI.** No `.github/workflows/` or other CI config — tests, `cargo fmt`,
  `cargo clippy`, and builds are not run automatically.
- **No doctests** — public APIs (`geometry`, `ShellCommand`, `AppCatalog`) have
  no runnable `///` examples.

## Recommended First Steps

1. Add `[dev-dependencies]`: `tempfile`, `rstest`, `proptest`.
2. Extract the pure helpers in **Highest-ROI Targets** into free functions so
   they can be unit-tested without GPU/Wayland.
3. Add unit tests for `app_catalog` parsing, `windows`/`rendering` rect math,
   `shell_integration` command buffering, and `accessibility` matchers.
4. Add a minimal CI workflow running `cargo test`, `cargo fmt --check`, and
   `cargo clippy`.
