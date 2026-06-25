# Hearthspace Technical Improvements

A running list of improvements identified during a technical review of `src`.

## Correctness / Robustness

- **Tighten `request_mode` mode handling.** The wildcard arm silently coerces
  unknown decoration modes to server-side.

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
