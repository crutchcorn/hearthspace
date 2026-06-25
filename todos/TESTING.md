# Automated Testing Report

A survey of the current state of automated testing in Hearthspace and where it
needs to grow. The goal is to identify the highest-ROI places to add tests and
the infrastructure work required to make harder areas testable.

> **Status:** the first round of recommendations from this report has been
> implemented — the suite grew from 20 to **58 tests**, `[dev-dependencies]`
> and CI are in place, and the highest-ROI pure helpers have been extracted and
> covered. Remaining work is now concentrated in the integration/headless areas
> (see [Hard-to-Test Areas](#hard-to-test-areas) and
> [WAYDRIVER.md](./WAYDRIVER.md)).

## Summary

- **58 tests** total across **8 inline `#[cfg(test)]` modules**, all passing.
- Coverage now spans the pure/extractable surface of `geometry`, `idle` (core),
  `app_catalog`, `command`, `windows` rect math, `rendering` close-button
  layout, `shell_integration` command framing, and `accessibility` matchers.
- `proptest` and `rstest` are in use (parameterised cases + invariant
  properties), alongside `tempfile` for filesystem-backed tests.
- The live runtime surface — the compositor event loop, GPU `render_frame`,
  Smithay input handling, viewport animation, the GPUI bar, and the AT-SPI tree
  walk — still has **no** automated coverage; these need an integration/headless
  harness.

## Current Coverage

| File | Tests | What is covered |
|---|---|---|
| [src/shell/app_catalog.rs](../src/shell/app_catalog.rs) | 21 | `split_exec` (quotes/escapes/unterminated-quote error), `unescape_desktop_value`, `parse_semicolon_list`, `desktop_entry_id`, `token_score` ranking tiers, `load_from_data_dirs` via tempdir (NoDisplay filtering, cross-dir id dedup), plus the original hidden/`OnlyShowIn`/field-code cases |
| [src/compositor/idle.rs](../src/compositor/idle.rs) | 8 | `IdleTrackerCore` state machine: scheduling, threshold cascades, per-window independence, activity reset, stale-generation rejection, deadline firing order |
| [src/geometry.rs](../src/geometry.rs) | 6 | `rect_contains` boundaries, canvas↔screen round-trip, zoom-around-point, zoom clamping, `ease_out_cubic`, interpolation clamping |
| [src/accessibility.rs](../src/accessibility.rs) | 6 | `accessible_matches_term`, `accessible_matches_window`, `is_desktop_shell_root`, `indent` |
| [src/compositor/shell_integration.rs](../src/compositor/shell_integration.rs) | 6 | `take_complete_commands` newline framing, `parse_command_line`, `is_valid_snap_instance_name`, `sanitized_path_component` |
| [src/compositor/windows.rs](../src/compositor/windows.rs) | 5 | `normal_insert_index_for_kinds` z-order, `title_bar_canvas_rect_for`, `close_button_canvas_rect_for`, `content_canvas_origin_for` |
| [src/compositor/rendering.rs](../src/compositor/rendering.rs) | 4 | `close_button_x_rects` layout, including `proptest` containment/symmetry invariants |
| [src/shell/command.rs](../src/shell/command.rs) | 2 | Wire-name round-trip, `launch-app` id parsing |

## Per-Module Assessment

| Module | Coverage | Testability | Priority | Remaining tests |
|---|---|---|---|---|
| `geometry.rs` | Good | Pure | Low (maintain) | Optional: more property tests for round-trip / clamp invariants |
| `shell/command.rs` | Partial | Pure | Low–Med | Negative-parse cases, whitespace, unknown verbs, `label`/`Display` |
| `shell/app_catalog.rs` | Good | Mostly pure | Low (maintain) | `terminal_command_for` / `preferred_terminal` selection paths |
| `compositor/idle.rs` | Good (core) | Pure core, threaded shell untested | Med | `WindowIdleDaemon` thread + `mpsc` + clock wrapper |
| `compositor/windows.rs` | Good (rect math) | Rect math pure, hit-test entangled | Med | Hit-testing against live `App` state, `raise_window` ordering |
| `compositor/rendering.rs` | Partial | GPU-bound; pure helper covered | Med | `render_frame` needs a GPU/headless harness |
| `compositor/viewport.rs` | None | Entangled with `App` + `Instant::now` | **Med** | Extract animation progress to pure fn, then test easing/snapping |
| `compositor/input.rs` | None | Needs Smithay seat | Low–Med | Extract scroll/keysym helpers; or seat-replay integration test |
| `compositor/shell_integration.rs` | Good (parsing) | Parsing pure; spawning is FS/process I/O | Med | Spawn path / snap symlinks / GTK config via tempdir + injected spawn |
| `compositor/mod.rs` | None | Very hard (protocol + winit + GL) | Low | Headless integration harness only — see WAYDRIVER.md |
| `accessibility.rs` | Partial | Matchers pure; tree walk is async D-Bus | Med | Tree walk needs a mock connection or live AT-SPI peer |
| `shell/bar.rs` | None | GPUI runtime | Low | Extract result/selection logic; test if decoupled |
| `test_apps/gtk.rs` | None | GTK runtime | Low | Fixture app — not worth testing |
| `config.rs` / `main.rs` / `lib.rs` | None | Trivial / arg dispatch | Low | Optional arg-dispatch test if dispatch is extracted |

## Highest-ROI Targets

The pure/near-pure functions identified as the first wave have now been
extracted and covered:

1. ✅ **`app_catalog.rs` parsing internals** — `split_exec`,
   `unescape_desktop_value`, `parse_semicolon_list`, `desktop_entry_id`, and
   `token_score` ranking, plus tempdir-based `load_from_data_dirs`.
2. ✅ **`rendering.rs::close_button_x_rects`** — pure rectangle layout, with
   `proptest` invariants.
3. ✅ **`windows.rs` rect geometry** — `title_bar_canvas_rect_for`,
   `close_button_canvas_rect_for`, `content_canvas_origin_for`, and
   `normal_insert_index_for_kinds` z-order logic (extracted to free functions).
4. ✅ **`shell_integration.rs` command buffering** — `take_complete_commands`
   framing, `parse_command_line`, `sanitized_path_component`, and snap-instance
   validation.
5. ✅ **`accessibility.rs` matching helpers** — `accessible_matches_term`,
   `accessible_matches_window`, `is_desktop_shell_root`, `indent`.

Still outstanding (next wave, same "extract then test" pattern):

6. ⬜ **`viewport.rs` animation math** — extract progress→(offset, scale) into a
   free function and test easing, clamping, and endpoint snapping at
   `progress >= 1.0`.
7. ⬜ **`input.rs` scroll/keysym helpers** — extract the pure event-translation
   bits (e.g. `axis_frame_from_event`) out from the Smithay-seat-dependent path.

## Hard-to-Test Areas

These require an abstraction layer or a headless integration harness rather than
unit tests. The planned path for most of them is the WayDriver-based headless
E2E harness in [WAYDRIVER.md](./WAYDRIVER.md).

- **`compositor/mod.rs`** — `run_winit` owns a calloop event loop, winit GL
  backend, Wayland display, sockets, and a spawned subprocess. Needs a headless
  Wayland client harness.
- **`rendering.rs::render_frame`** — requires a real `GlesRenderer` +
  framebuffer (GPU/EGL). Only the geometry helper is unit-testable; a headless
  GLES path would unlock screenshot-based assertions.
- **`input.rs::handle_input_event`** — depends on Smithay pointer/keyboard
  handles and `WinitInput` events; needs a seat abstraction or event-replay
  integration test. A headless backend could inject synthetic seat events.
- **`bar.rs` / `test_apps/gtk.rs`** — require GPUI / GTK runtimes; suited to
  end-to-end UI testing. Note GPUI ships no AccessKit, so the bar exposes no
  AT-SPI tree (see WAYDRIVER.md) — drive it via the command socket + screenshots.
- **`accessibility.rs` tree walk** — requires a live AT-SPI D-Bus registry;
  needs a mock connection trait or an integration test against a known peer. A
  headless harness running a real client gives this for free.
- **`shell_integration.rs` spawning / snap symlinks / GTK config** — touch the
  real filesystem and `Command::spawn`; testable only with tempdir sandboxing
  (`XDG_*` overrides) and dependency injection for the spawn step.
- **`idle.rs::WindowIdleDaemon`** — the core is tested, but the thread + `mpsc`
  + real `Instant`/`recv_timeout` wrapper is not; needs a clock abstraction or a
  timing-tolerant integration test.

## Infrastructure

Resolved since the initial report:

- ✅ **`[dev-dependencies]`** in [Cargo.toml](../Cargo.toml): `proptest`,
  `rstest`, `tempfile` — in use for parameterised cases, invariant properties,
  and filesystem-backed tests.
- ✅ **CI** — [.github/workflows/ci.yml](../.github/workflows/ci.yml) runs
  `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test --all-targets`, plus a `cargo-deny` job for advisories/bans/
  sources. A centralized `[lints]` table and `rust-toolchain.toml` keep local
  and CI checks aligned.

Still open:

- **No `[features]`** to gate hardware/headless code paths for testing — a
  `headless` path is the keystone for the integration harness (WAYDRIVER.md).
- **No `tests/` integration directory** and **no `benches/`**, despite
  performance-sensitive paths (damage tracking, render element collection, idle
  deadline compaction).
- **No doctests** — public APIs (`geometry`, `ShellCommand`, `AppCatalog`) have
  no runnable `///` examples.

## Next Steps

1. Extract and unit-test the remaining pure helpers (`viewport` animation math,
   `input` event-translation helpers).
2. Stand up the headless E2E harness per [WAYDRIVER.md](./WAYDRIVER.md) to unlock
   the runtime surface (compositor loop, render, input, AT-SPI tree walk).
3. Add tempdir-sandboxed tests for the `shell_integration` spawn path, and a
   clock abstraction for the `WindowIdleDaemon` thread.
4. Consider doctests on the stable public APIs and a `benches/` baseline for the
   damage/render-element hot paths.
