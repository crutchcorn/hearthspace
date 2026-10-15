# Shell controls

The shell bar displays the clock and four compact icons: Search, Notifications,
Canvas map, and Power. Search, notifications, the map and the power menu each
open a separate overlay or popover. Opening one closes the previous panel.
Escape, the close icon, an outside click, or clicking
the active toolbar icon dismisses it.
Panels have rounded transparent corners and shrink to fit their content. Long
lists scroll within the available height; search keeps its heading and input
visible above the results. Search uses 60% of the output's logical width, with a
360-pixel minimum where space permits, capped to leave 12-pixel side margins
on small displays.

Tap Meta (also called Super or the Windows key) to toggle the search overlay.
The action occurs when the key is released. Using Meta in a shortcut, such as
Super+Return, Super+L or Super+arrow, does not toggle search afterward.
Super+Space opens search, as does the Search icon. Typing searches application
names and running-window titles or app IDs. The first result is selected initially;
Up and Down move the visible selection, and Enter activates that result. The
search field retains typing focus while the selection moves.
Selecting a running window focuses its existing ID and brings it into view.
An empty query shows a typing hint. Hearthspace Settings is available through
search. Prebuilt sessions register its desktop entry in their private runtime,
using the selected binary directory when provided. Native agent startup builds
and verifies the Settings executable as part of its required binary set.

The notification bell opens the notification list; its dot indicates active
notifications. The [canvas map](CANVAS_MAP.md) remains beside the bell, with
viewport dragging and zoom controls. Power contains Lock and Log out. Log out
shows a confirmation with a Cancel option before ending the session.

The former Reset view action, Super+0 shortcut and `reset-view` command are
removed. There is no Overview button, separate Windows button or unfiltered
window-list mode. Overview gestures and Super+O, Alt+Tab, and query-based window
search remain available. Settings, Lock and Log out are no longer standalone
bar buttons.

## Agent control

Buttons retain descriptive accessible names despite showing icons: `Search`,
`Notifications`, `Canvas map`, and `Power`. Close controls are named `Close
search`, `Close notifications`, `Close canvas map`, and `Close power menu`.
The power choices are `Lock`, `Log out`, `Confirm log out`, and `Cancel`.
Search rows retain button roles and the names `Open: APP NAME` or `Window:
WINDOW TITLE`; the current result exposes the accessibility `selected` state.

Unlocked `status.shell_panels` lists visible panel bounds as `kind`, `output`,
`x`, `y`, `width`, and `height`. Kinds are `search`, `notifications`, `map`, and
`power`. Bounds use global logical output coordinates. AccessKit bounds are
physical and local to each surface, so pointer automation combines both with
the output scale. Shell panels never enter the normal `windows` list.

`focus-launcher` explicitly opens search while a synthetic bare Meta tap toggles
it. The independent `zoom-to SCALE` and `pan-to X Y` control commands allow test
fixtures to set an explicit viewport. Wait for zoom animation to settle before
panning, since a direct pan interrupts animation. Neither command changes
application canvas positions.

## Acceptance

Build native and E2E binaries, then run the isolated headless acceptance:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
HEARTHSPACE_E2E=1 python3 -m unittest discover -s scripts/tests -p test_shell_headless.py -v
```

The fixture owns its compositor, shell, notification service and private
accessibility bus. It posts notifications through the standard D-Bus `Notify`
method with `gdbus`; native acceptance uses `notify-send`. This avoids changing
Ubuntu's `notify-send` AppArmor policy for the temporary headless bus path.
`HEARTHSPACE_SHELL_EVIDENCE=/tmp/hearthspace-shell-headless-1`
retains its screenshots, status and semantic observations outside temporary
runtime cleanup. The main workflow uses a 2560×1600 output at scale 2. A second
800×600 output at scale 1 checks popup geometry and pixels, storing evidence in
the `small-output` subdirectory. Choose a fresh evidence directory for each attempt.

The same assertions run on real hardware through the existing native harness:

```sh
scripts/shell-smoke --session /tmp/hearthspace-shell-native-1
```

Follow the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and [native testing](NATIVE_TESTING.md) for seat ownership and recovery. The
workflow starts only an owned spare-VT session and restores its recorded original
VT. It checks icon ordering and removed controls; Meta release immediately
followed by typing, including exact accessible text and no leakage to the prior
terminal; Up/Down result selection and Enter activation of a running window and
Settings; pointer selection; Settings close; notifications; and power
confirmation cancellation. It uses Tab to focus Lock without activating it, then
focuses Cancel and presses Enter to dismiss the logout confirmation. It never
confirms logout. It first launches an owned dark terminal backdrop, maximizes it
below the bar, and compares every popup corner against its captured background.
It verifies a visible popup interior and measures
the gap below the final accessible content. Search highlight assertions require
both the old and new rows to change visibly. Screenshots and cleanup evidence are
retained on failure. The backdrop is closed before the application workflows.

### Hardware evidence, 2026-10-15

The full workspace passed 570 Rust tests with native, E2E and test-app features,
298 Python tests with E2E enabled, warnings-denied Clippy, Rustfmt, Ruff, and
the full build. `/tmp/hearthspace-shell-headless-settings` retains the final
headless shell workflow, including the Settings launch. Earlier
`/tmp/hearthspace-shell-headless-1` retains a failed notification attempt blocked
by the host's `notify-send` AppArmor profile; the fixture now uses the explicit
private-bus D-Bus client described above, without changing host policy.

Tests used the physical GPD laptop, AMD Radeon 890M (`amdgpu`), kernel
`7.0.0-38-generic`, and eDP-1 at 2560×1600 with scale 2. The agent stayed in its
background service while each owned desktop used TTY7 and restored TTY2.

`/tmp/hearthspace-shell-map-native-1` passed live minimap updates, both zoom
buttons, pointer dragging, and touchscreen dragging through a temporary uinput
device. Page-flip screenshots, geometry assertions and an independent libinput
trace are retained. This was automated single-output acceptance, not a manual
physical-finger test. Cleanup confirmed that the service stopped, its runtime
was removed, and the original VT was active.

`/tmp/hearthspace-shell-native-1` retains the first shell attempt. Exact Meta
search text and no leakage into the terminal were observed, but the native
prebuilt session had no Settings desktop entry. The waiting test was interrupted
after preserving a screenshot and accessibility snapshot; owned-session cleanup
completed and restored TTY2.

`/tmp/hearthspace-shell-native-2` passed the complete shell workflow after the
Settings fix. Captures and semantic assertions confirm the four icons, exact
Meta query text with zero terminal leakage, Settings launch and close, running
window selection, notification delivery and dismissal, and keyboard cancellation
of logout. Cleanup reports `complete: true`; TTY2 was restored, the runtime was
removed, and the service has no remaining main process or cgroup. The stopped
unit retains a failed diagnostic because the supervisor attempted to stop an
already collected app-scope owner during shutdown; `session.log` and
`service-stop-journal.log` retain that separate cleanup-reporting issue.

Release `69c4718835bb` was installed with `scripts/install-session install`.
Installed compositor, shell, session and Settings hashes match the optimized
build. Existing sessions keep their running binaries; log out and log back in
to load the new bar.

### Popup and search evidence, 2026-10-15

The popup changes passed all 584 Rust tests and 314 Python tests, with native,
E2E, test-app and required accessibility checks enabled. The full workspace
build, warnings-denied Clippy, Rustfmt, Ruff lint and formatting also passed.
`/tmp/hearthspace-popup-headless-final` retains the headless acceptance and its
`small-output` case. Search measured 768 logical pixels wide on a 1280-pixel
output and 480 on an 800-pixel output. All four popup corners matched the dark
backdrop, with opaque interior pixels checked independently. Content had a
13-pixel bottom gap; clearing the search shrank it from 326 to 128 pixels high.
Up/Down visibly moved the selected row, and Enter focused the selected existing
window without changing the set of open windows.

The first headless attempt, `/tmp/hearthspace-popup-headless-1`, could not prove
transparency against its white backdrop; the fixture now supplies an owned dark
terminal. The second attempt exposed an AccessKit interface-removal failure
when clearing results after the popup grew. A semantic group around each
scrollable popup body fixes the accessible hierarchy. Regression tests now feed
the changing trees through the real AT-SPI adapter, including result removal.
`/tmp/hearthspace-popup-headless-2` retains that failure; subsequent runs passed.

Native acceptance used the same physical GPD laptop, AMD Radeon 890M (`amdgpu`),
kernel `7.0.0-38-generic`, and eDP-1 at 2560×1600 with scale 2. Both owned sessions
used TTY7 and restored the user's existing Hearthspace session on TTY3.
`/tmp/hearthspace-popup-native-1` passed the complete shell workflow, including
rounded-corner pixel checks, content sizing, visible arrow-key selection,
search clearing, Enter activation, Settings launch, notifications and logout
cancellation. `/tmp/hearthspace-popup-map-native-1` passed live minimap updates,
both zoom buttons, pointer dragging and temporary-uinput touchscreen dragging.
These are automated native tests, not manual physical-finger acceptance.

Both runs retain page-flip screenshots and semantic observations. Cleanup
reported all steps complete with no errors; both services are inactive with
`Result=success`, no main process or cgroup, and their runtimes removed. TTY3
was confirmed active after each run.

Release `752a8d9aad4c` was installed with `scripts/install-session install`.
All eight installed executables match both the optimized build and installation
manifest hashes. The user's running session was preserved; log out and back in
to load these popup and search changes.
