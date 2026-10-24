# GTK touchscreen window movement

`scripts/gtk-touch-smoke --session /tmp/hearthspace-gtk-touch-1` covers GTK's own
client-side titlebar gestures in GNOME Calculator (GTK 4) and an ordinary GTK 3
HeaderBar fixture. Each app uses the owned private Wayland display and session
bus, isolated XDG configuration/data/cache directories, and memory-only GSettings.
It checks two held-contact movement checkpoints, another drag after release,
and pointer movement after the finger lifts, at canvas scales 1.0 and 0.8.
Rendered content must follow the window and its vacated pixels must repaint.
An owned dark terminal backdrop supplies contrast for those movement checks;
light GTK content can otherwise resemble correctly repainted wallpaper. Pixel
thresholds remain unchanged. Inspection and titlebar-maximize modes omit it.
Touch taps then compute `2 + 3 = 5` in Calculator and increment the GTK 3 counter;
private AT-SPI results and screenshots establish actual application input.
Both clients, the input device, and the native session are cleaned up on failure.
The run retains the isolated app state and logs as evidence.

Contacts come from an owned uinput touchscreen through libinput on a real VT.
This verifies native compositor/client behavior and presentation; physical finger
feel and multi-output interaction still require separate testing.

This test requires `gnome-calculator`, Python GTK 3/PyGObject, and the
same uinput/libinput dependencies as the other touch suites. It expects one
output at least 960×800 logical pixels. Use `--case calculator` or `--case gtk3`
to isolate one toolkit, or `--inspect-only` to capture initial windows and their
accessibility trees without injecting input. Build the native binaries first
and run it separately from all other native or GPU-intensive suites.

## Regression and hardware evidence

### Native Wayland touch delivery

The 2026-10-23 native run used a physical ThinkPad T480s, Intel UHD 620 (`i915`),
Linux `7.0.0-38-generic`, and eDP-1 at 1920×1080, output scale 1. Chrome 156 used
its default Wayland backend and a private profile without sandbox-disabling
flags. Its page telemetry recorded touch scrolling of 562 pixels at canvas scale
1 and 349 at scale 0.5, both with zero selected characters. The mouse control
selected 542 characters without scrolling. These are telemetry measurements;
captures corroborate the visible behavior. Input came from owned uinput devices
through libinput, not physical fingers. X11 retains pointer fallback.

The same compositor build passed 44 GTK 3, 44 GTK 4, and 20 generic titlebar
checkpoints. Calculator and the GTK 3 movement fixture then passed all 16 held
drag checkpoints at scales 1 and 0.8, with content taps and subsequent input
recovery. Movement captures matched at least 99.49% of translated content samples
and 100% of vacated samples. Both sessions restored tty2 and removed their owned
clients, devices, and runtimes; the units were inactive with no process or cgroup.

Evidence is under `/home/crutchcorn/hearthspace-chrome-touch-npd9eano`:
`CHROME-FINAL.json`, `GTK-FINAL.json`, and `GTK-MOVEMENT-FINAL.json` record final
results and provenance. The initial `final-gtk-movement` attempt failed because
light GTK pixels resembled the repainted wallpaper. The contrast-backdrop rerun
in `final-gtk-movement-contrast` passed with unchanged pixel thresholds; it does
not establish a compositor rendering defect in the earlier attempt.

The final product passed formatting, Clippy, build, both Rust configurations
(1,836 E2E and 1,534 CI tests), 571 Python E2E tests, Ruff, and dependency checks
(with cargo-deny warnings). After the movement-fixture adjustment, all 50 focused
GTK tests and the full 575-test Python unit run passed, with 14 opt-in E2E tests
skipped in that unit run. Existing stale-copy rejection remains covered. These
runs validate the checkout build, not an installed session that has not restarted.

### CSD maximize and titlebar gestures

After building, start an isolated native session using the
[native control workflow](../.agents/skills/hearthspace-native-control/SKILL.md).
The titlebar acceptance path attaches to that session; its caller must stop it
and verify recovery afterward:

```sh
scripts/gtk-touch-smoke --session "$native_run" --titlebar-maximize \
  --window-test "$PWD/target/debug/hearthspace-window-test" \
  --timeout 450 --total-timeout 1200
```

This opt-in path uses isolated GTK 3/4 HeaderBar fixtures. Pointer double-clicks
and uinput touchscreen double-taps must canvas-maximize/restore; their maximize
buttons must enter focused view with the application header retained and the
canvas frame hidden. An explicit header gesture exits focused view into canvas
maximization. The ordinary F9 maximize request in `hearthspace-window-test`
exercises the generic fallback. Super+M, Super+Shift+F and application F11 retain
their separate actions. The workflow checks committed client state, original
floating geometry on restore, rendered input and fullscreen signals at canvas
scales 1.0 and 0.8. It records fresh target geometry and page-flip captures.
Each GTK application exercises 44 presentation transitions with input and pixel
checks. Allow 450 seconds per application and 1,200 seconds overall; the owning
session's watchdog must also cover startup and inspection (for example, 1,500
seconds). These are acceptance-run budgets, separate from compositor IPC deadlines.

Use `--case gtk3`, `--case gtk4` or `--case generic` for a focused run. This
validates cooperating GTK headers and generic maximize requests, not custom
titlebar implementations in every application. See the
[GTK protocol contract](../crates/hearthspace-compositor/protocols/README.md)
for input validation and compatibility limits.

The CSD matrix passed on 2026-10-22 on a physical ThinkPad T480s with Intel
UHD 620 graphics (`i915`), Linux `7.0.0-38-generic`, and `eDP-1` at 1920×1080,
output scale 1. GTK 3.24.52 and GTK 4.22.4 each passed all 44 transitions; the
generic maximize fixture passed 20. Each transition checked committed state and
rendered application input. Focused entry also checked that every live shell
widget disappeared, before deliberate hover reveal and again afterward.

Evidence is under `/home/crutchcorn/hearthspace-csd-recovery-xm_vd6xe/`:
`native-gtk-fdkfdpm5` contains the completed GTK 3 workflow;
`native-gtk-q6zdwmkg` contains the completed GTK 4 and generic workflows.
The latter retained separate touchscreen traces for each pointer, touch, and
keyboard batch. Both owned sessions restored tty2, removed their runtimes and
clients, and left inactive units with PID 0 and no cgroup. No other native or
GPU-intensive suite ran concurrently.

Earlier attempts exposed acceptance-helper defects: the temporary touchscreen's
180-second guard expired during an oversized batch, GTK 4 retained the accessible
label **Maximize** while maximized, and a client header border confused a
whole-widget background estimate. Fresh devices per batch, toolkit-specific
control discovery, and per-row background sampling address these cases without
relaxing presentation or input assertions. Synthetic regressions still reject
shell glyphs retained over a disappearing backdrop. The final comparator also
passed all 27 saved initial/re-hide observations from the earlier GTK runs.
This evidence validates the worktree build; it does not update the installed
Hearthspace release.

Automated validation passed rustfmt, Clippy with warnings denied, the full
workspace/all-target build, and 1,584 Rust tests with `udev,e2e,test-apps` and
required accessibility enabled. The full Python E2E run passed all 512 tests.
After adding the native observer regressions, the final Python unit run completed
566 tests with only the 14 opt-in E2E cases skipped; Ruff lint and formatting
passed for all 191 Python files. Logs are retained in the same evidence root's
`validation-6gh7jrje` directory. Native Firefox acceptance is recorded in
[daily application testing](DAILY_APPS_TESTING.md).

### Integration with compositor-owned CSD grabs

Rebase validation on 2026-10-23 retained the upstream move/resize grab and focus
recovery behavior from `eb5e391`. GTK authority tests now require a consumed
release after compositor takeover, followed by pointer re-entry and a usable
new click. A real-client regression keeps a dialog's original touch drag alive
through its parent's maximize and restore requests.

The combined build passed rustfmt, Clippy with warnings denied, the full
workspace/all-target build, and 1,661 Rust tests across 39 targets. The full
566-test Python E2E run had one observer failure: a quiet intermediate audio
value was mistaken for the final slider position. The repaired observer waits
for the explicit 75% endpoint in both backend state and visible text before
testing Right to 76%. Its regression fails with the former observer, all three
new cases pass, and the complete controls workflow passed its isolated rerun
with the original deadline. The other 565 E2E cases passed unchanged. The final
unit run completed 569 tests with only the 14 opt-in E2E cases skipped; Ruff
lint and formatting passed for all 192 Python files.

On the same Intel/i915 host, `native-gtk-o006m9ss` repeated all 108 GTK 3, GTK 4,
and generic transitions successfully. Cleanup restored tty2 and removed the
owned clients, devices, runtime, and service processes. Evidence is beneath the
same recovery root: `validation-oeqz75j_` contains full checks, and
`rebase-kds6v9b7` contains the observer negative control and passing rerun.

### Earlier touch movement repair

GTK can send `xdg_toplevel.unset_fullscreen` immediately before its titlebar
`move` request, even when the window is already floating. Hearthspace previously
treated this redundant request as a state change, cancelling the touch-emulated
pointer press before Smithay could validate the move. Redundant presentation
requests now acknowledge the client without interrupting input or replacing
pending geometry. Presentation changes affecting the focused window stop canvas
pinch handling while preserving a delivered touch pointer sequence, including
one moving a transient dialog. Background maximize requests wait for focus before
changing canvas interaction. Move/resize serial, seat, and window validation
remain intact.

The failure was reproduced with GNOME Calculator 50.0 (GTK 4.22.4) on the physical
GPD laptop, Linux 7.0.0-38-generic, AMD Radeon 880M/890M with amdgpu, and eDP-1 at
2560×1600, scale 2. `/tmp/hearthspace-gtk-touch-before` retains the Wayland trace:
GTK sent restore and move with the valid press serial; the compositor immediately
released the pointer while the injected contact remained held. Window geometry
and screenshots stayed stationary through both movement checkpoints.

Final acceptance passed in `/tmp/hearthspace-gtk-touch-fixed-4` with Calculator
and GTK 3.24.52. All 16 held-contact checkpoints passed at canvas scales 1.0/0.8,
with 99.09–100% coherent content matches and at least 99.71% repainted vacated
pixels. Each app supported a second drag, stopped following after finger lift,
and received content taps: Calculator displayed `5`, and the GTK 3 counter
reached `2`. All 39 captures retained native page-flip metadata at output scale 2.
The original tty3 was restored, the owned unit was inactive with PID 0 and no
cgroup, and both app processes, all temporary touch devices, and the runtime
directory were removed. No other native or GPU-heavy test ran concurrently.

`/tmp/hearthspace-gtk-touch-fixed-1` passed Calculator at both canvas scales and
GTK 3 dragging, then failed in the test helper's content-target coordinates.
GTK 3 includes shadow margins in AT-SPI window coordinates. The helper now derives
the geometry origin from the fixture's accessible HeaderBar; a regression uses
the observed asymmetric margins. `/tmp/hearthspace-gtk-touch-fixed-2` passed both
complete app workflows, but screenshot/log review exposed a missing Cairo foreign
converter in the fixture's decorative drawing callback. Its marker now uses plain
GTK widgets, removing that dependency. Both attempts retain their evidence and
completed cleanup; the second run is not the final fixture acceptance.

The third run, `/tmp/hearthspace-gtk-touch-fixed-3`, passed Calculator and GTK 3
at scale 1, then rejected the final zoomed GTK 3 screenshot comparison despite
correct movement geometry. All 4,028 distinctive samples matched at one coherent
translation two physical pixels from the rounded status displacement. Pixel
evidence now searches within one output-logical pixel of that displacement,
requiring the same translation for every sample, with unchanged match and
vacated-region thresholds. Regressions reject movement outside that rounding
budget, stationary pixels, and stale copies. Cleanup restored the recorded tty3;
the earlier attempts restored tty2. Every owned client/device was removed, the
units had no main process or cgroup, and all private runtimes were gone.

The fix is installed as optimized release `39f9259b482d`; all eight current
binaries match `target/release`, and all 112 installation-manifest hashes were
verified. Existing desktop sessions must restart to use the new compositor.

Final checks passed: 618 Rust tests with E2E and required accessibility enabled,
329 Python tests with E2E enabled, rustfmt, Clippy with warnings denied, Ruff
lint/format checks, the full workspace/all-target build with `udev,e2e,test-apps`,
and the optimized installation build. Logs are retained under
`/tmp/hearthspace-gtk-touch-{rust-tests,python-tests-final,clippy,build,install}.log`.

