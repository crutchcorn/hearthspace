# Shell controls

The default layout places independent native controls in four corners on each output:

- **Top left — Notifications:** one button combines the bell with “All clear” or the number of new notifications and opens the notification panel.
- **Top right — System tray:** application tray icons, clock/calendar, Wi-Fi, brightness, sound, battery when available, and Power. Tray icons appear only while apps publish them; see [application tray](TRAY.md).
- **Bottom left — Canvas tools:** [desktop-item selection](DESKTOP_ITEMS.md), [canvas search](CANVAS_SEARCH.md), window-group selection, Change layout, and the [canvas map](CANVAS_MAP.md).
- **Bottom right — App dock and Apps List:** running apps and the installed-app browser.

The dock groups windows by application ID. Activating an icon focuses and reveals
an existing window on the infinite canvas; repeated activation cycles through that
app's windows. The active app is highlighted, dots indicate running windows, and
icons update as windows open or close. Apps without desktop icons have a window
symbol. A page button keeps every running app reachable when space is limited.

Each panel opens beside its control. Top attachments open downward, bottom
attachments upward, and side attachments inward. Canvas controls use the side
with more room; popups stay within the output. Opening one closes the previous panel.
Escape, the close icon, an outside click, or the active corner button dismisses it.
Panels have rounded transparent corners and shrink to fit their content. Long
lists scroll within the available height; Apps List keeps its heading, input,
view switch, and footer visible. It requests 650 logical pixels, capped to the output with 24-pixel side margins. Maximized apps fit
between the top and bottom controls; fullscreen retains its existing shell policy.

Tap Meta (also called Super or the Windows key) to toggle Apps List. The action
occurs on release. Using Meta in a shortcut, such as Super+Return, Super+L or
Super+arrow, does not toggle it afterward. Super+Space opens Apps List, as does
its dock button or an upward touchscreen swipe starting on the dock.

Ctrl+K or Super+K opens canvas search, which temporarily arranges matching desktop
objects and live windows around a native search field. Its toolbar button arms
click-to-open Search mode. This is independent of Apps List.

Alt+Tab opens the [live window grid](WINDOW_MANAGEMENT.md#held-window-switcher),
with an accent outline and a glass title badge below the selected preview.
Alt release commits; Shift reverses cycling and Escape cancels.

Super+F6 moves keyboard focus through Notifications, System tray, Canvas tools,
and App dock on the active output. Super+Shift+F6 moves backward. Tab navigates
through the independent controls in a group; Enter activates the focused control. Escape closes an open panel
or returns from the corner to the most recent app without moving the canvas.

An empty query shows installed applications alphabetically in a four-column icon grid.
The view switch selects an icon grid or a list with app descriptions; narrow
panels reduce the grid column count. The Draw tab reuses the existing favorite
gesture pad and matching flow. Typing filters apps
and running-window titles or IDs. Up and Down move between rows (Left and Right move between grid cells); Enter
activates it while the search field retains typing focus. Selecting a running
window focuses its existing ID and brings it into view. Hearthspace Settings is
available in Apps List. Prebuilt sessions register its desktop entry in their
private runtime, using the selected binary directory when provided.

Power contains Lock and Log out; Log out requires confirmation and offers Cancel.
Overview gestures, Super+O, and Alt+Tab remain available. Settings, Lock and Log
out are reached through Apps List and Power rather than standalone corner buttons.

Wi-Fi shows the network tray handoff until a network-management backend is
available; it does not invent nearby networks or connection state.
[Brightness and Sound](AUDIO_AND_BRIGHTNESS.md) open separate slider dropdowns
using the existing hardware readings. Battery details appear in the Battery
dropdown. Missing hardware has an explicit unavailable state. The clock dropdown
shows the current month.

## Change layout

Open **Change layout** from Canvas tools, canvas Search, or Settings → System.
Select a control, desktop item, window, or group, then use its editor to choose
Background, Canvas, or Frame. Background and Canvas follow the viewport; Frame
stays attached to an output edge. Wi-Fi, clock, sound, brightness, battery,
application tray and power can be separated from the default system group.
Canvas tools and the app dock remain composite controls.

At widths of 760 logical pixels or less, unchanged default groups share full-width
top and bottom bars. Custom placements remain intact. Canvas tools collapse to a
menu button. Long groups scroll along their edge, and keyboard focus reveals the
focused control without moving the canvas.

Drag toward an edge or one of its corner guides to attach an item. Hold Alt to
align the along-edge position with the canvas grid. Dropping onto another frame
item forms an ordered group. Separate releases one member; Ungroup releases all.
Moving a group onto Canvas or Background releases its members at their displayed
positions. Removing a group hides its controls and returns its windows and desktop
items to the canvas; it does not close applications or delete files.

Pinned windows retain their live application buffers. Their edge strip expands
on hover and opens fully when activated. Focusing elsewhere collapses it. Search,
Alt+Tab and dock activation reveal the existing pinned window without panning the
canvas. A close request prompts for confirmation. Desktop items become compact
frame tiles, retaining their normal activation and accessible names.

Done and Escape retain completed edits. Escape during a drag restores the drag's
starting layout and exits editing. Tab and Shift+Tab navigate native editor
controls, with pressed placement buttons, radio choices for clock formats, and
keyboard focus rings. Drawing is not part of the layout editor.

Layout, detail visibility, clock format and group membership persist per output.
Reopened windows recover attachments through their existing application slots.
Settings → System can permit hiding Canvas tools and restore them. Until that
permission is enabled, removing the tools opens an explanation with a Settings
link. Search remains a way to find and restore hidden controls.

## Agent control

Buttons retain descriptive accessible names despite showing icons: `Apps List`,
`Notifications`, `Search canvas`, `Canvas map`, `Select window group`, `Wi-Fi settings`,
`Brightness settings`, `Sound settings`, `Date and time`, and `Power`.
Close controls include `Close apps list`, `Close notifications`, `Close canvas map`,
and `Close power menu`. Dock names include the app, window count, active state,
and next window when cycling.
The power choices are `Lock`, `Log out`, `Confirm log out`, and `Cancel`.
Search rows retain button roles and the names `Open: APP NAME` or `Window:
WINDOW TITLE`; the current result exposes the accessibility `selected` state.

Unlocked `status.shell_corners` describes legacy corner surfaces when present, with
`corner`, `output`, `x`, `y`, `width`, and `height`. The current shell uses independent
frame surfaces instead. Use `layout-status` for their measured bounds, orientation,
visibility and group identity, plus pinned windows, editor selection and animation
state. These bounds use global output-logical coordinates.

Unlocked `status.shell_panels` lists visible panel bounds as `kind`, `output`,
`x`, `y`, `width`, and `height`. Kinds are `search`, `notifications`, `map`,
`power`, `controls`, and `tray`. Bounds use global logical output coordinates. AccessKit
bounds are physical and local to each surface, so pointer automation combines both with
the output scale. Shell panels never enter the normal `windows` list.

`focus-launcher` explicitly opens Apps List while a synthetic bare Meta tap toggles
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
VT. It checks corner placement and control names; Meta release immediately
followed by typing, including exact accessible text and no leakage to the prior
terminal; Super+F6 corner traversal and keyboard panel activation; repeated dock
activation cycling existing windows; Up/Down result selection and Enter activation of a running window and
Settings; pointer selection; Settings close; notifications; and power
confirmation cancellation. It uses Tab to focus Lock without activating it, then
focuses Cancel and presses Enter to dismiss the logout confirmation. It never
confirms logout. It first launches an owned dark terminal backdrop, maximizes it
between the controls, and compares every popup corner against its captured background.
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
