# GTK touchscreen window movement

`scripts/gtk-touch-smoke --session /tmp/hearthspace-gtk-touch-1` covers GTK's own
client-side titlebar gestures in GNOME Calculator (GTK 4) and an ordinary GTK 3
HeaderBar fixture. Each app uses the owned private Wayland display and session
bus, isolated XDG configuration/data/cache directories, and memory-only GSettings.
It checks two held-contact movement checkpoints, another drag after release,
and pointer movement after the finger lifts, at canvas scales 1.0 and 0.8.
Rendered content must follow the window and its vacated pixels must repaint.
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

GTK can send `xdg_toplevel.unset_fullscreen` immediately before its titlebar
`move` request, even when the window is already floating. Hearthspace previously
treated this redundant request as a state change, cancelling the touch-emulated
pointer press before Smithay could validate the move. Redundant presentation
requests now acknowledge the client without interrupting input or replacing
pending geometry. Actual state/output changes retain their cancellation path,
and move/resize serial, seat, and window validation remain intact.

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

