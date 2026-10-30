# Shell controls

The default layout places independent native controls in four corners on each output:

- **Top left — Notifications:** one button combines the bell with “All clear” or the number of new notifications and opens the notification panel.
- **Top right — System tray:** application tray icons, clock/calendar, Wi-Fi, brightness, sound, battery when available, and Power. Tray icons appear only while apps publish them; see [application tray](TRAY.md).
- **Bottom left — Canvas tools:** a chooser for [desktop-item selection](DESKTOP_ITEMS.md), [Pan canvas](WINDOW_MANAGEMENT.md#pan-canvas-with-a-mouse), and [canvas search](CANVAS_SEARCH.md), followed by window-group selection, Change layout, and the [canvas map](CANVAS_MAP.md).
- **Bottom right — App dock and Apps List:** favorite and running apps, plus the installed-app browser.

These positions are a starting arrangement. **Change layout** lets you keep a
folder at an edge, move the clock beside the dock, or place controls on the
canvas. An edge attachment stays in reach while you pan; an object on the canvas
moves with your work. Moving a control preserves its purpose and identity.

The dock shows favorites in saved order, followed by other running apps. Favorites
remain visible when closed; activating one launches it on that dock's output.
Windows matching the same installed desktop entry share one icon, including a
matching favorite. Activating a running app focuses and reveals an existing window
on the infinite canvas; repeated activation cycles through that app's windows.
The active app is highlighted, dots indicate running windows, and closed favorites
have no dot. Apps without desktop icons have a window symbol. A page button keeps
every app reachable when space is limited. Application tray icons remain in the
top system group with their original status menus, including for apps that also
appear in the dock.

Each panel opens beside its control. Top attachments open downward, bottom
attachments upward, and side attachments inward. Canvas controls use the side
with more room; popups stay within the output. Opening one closes the previous panel.
Escape, the close icon, an outside click, or the active control dismisses it.

Panels have rounded transparent corners and shrink to fit their content. Long
lists scroll within the available height; Apps List keeps its heading, input,
view switch, and footer visible. It requests 650 logical pixels, capped to the output with 24-pixel side margins. Maximized apps fit
between the top and bottom controls; fullscreen retains its existing shell policy.

Tap Meta (also called Super or the Windows key) to toggle Apps List. The action
occurs on release. Using Meta in a shortcut, such as Super+Return, Super+L or
Super+arrow, does not toggle it afterward. Super+Space opens Apps List, as does
its dock button or an upward touchscreen swipe starting on the dock.

Launcher requests reserve keyboard input immediately. The shell must acknowledge
a request within one second; that first acknowledgement grants five seconds to
create and map its first buffer. Repeated activation or acknowledgements cannot
extend the deadline. Cancellation, output/token matching, and the bounded input
queue still apply, so slow startup can preserve early typing without accepting a
stale or cancelled launcher. Canvas search retains its explicit session lifetime.

❖ + K opens canvas search, which temporarily arranges matching desktop
objects and live windows around a native search field. Its toolbar button arms
click-to-open Search mode. This is independent of Apps List.

Alt+Tab opens the [live window grid](WINDOW_MANAGEMENT.md#window-switcher),
with an accent outline and a glass title badge below the selected preview.
Alt release commits; Shift reverses cycling and Escape cancels.

Super+F6 moves keyboard focus through Notifications, System tray, Canvas tools,
and App dock on the active output. Super+Shift+F6 moves backward. Tab navigates
through the independent controls in a group; Enter activates the focused control. Escape closes an open panel
or returns from the corner to the most recent app without moving the canvas.

An empty query shows all installed applications alphabetically in a four-column
icon grid, with artwork centered above each name. Favorites appear in the dock;
pinning an app does not change its position or presentation in Apps List.
The view switch selects an icon grid or a list with app descriptions; narrow
panels reduce the grid column count. The Draw tab reuses the existing favorite
gesture pad and matching flow. Typing filters apps
and running-window titles or IDs. Up and Down move between rows (Left and Right move between grid cells); Enter
activates it while the search field retains typing focus. Selecting a running
window focuses its existing ID and brings it into view. Hearthspace Settings is
available in Apps List. Prebuilt sessions register its desktop entry in their
private runtime, using the selected binary directory when provided.

Power contains Lock screen, Suspend, Log off, Reboot, and Turn off. Log off,
Reboot, and Turn off open a centered confirmation with Cancel initially focused.
The backdrop blocks desktop interaction until dismissal; Enter initially
cancels, and Escape closes it. Power capabilities and failures come from the
native session service. See [power handling](POWER.md).
Pinch gestures and Alt+Tab open the window switcher. Settings, Lock and Log
out are reached through Apps List and Power rather than standalone corner buttons.

Wi-Fi directs you to a network tray application. Hearthspace does not currently
provide its own network scanning or connection controls.
[Brightness and Sound](AUDIO_AND_BRIGHTNESS.md) open separate slider dropdowns
using the existing hardware readings. Battery details appear in the Battery
dropdown. Missing hardware has an explicit unavailable state. The clock dropdown
shows the current month. Its **Date format** menu offers Time only, Weekday,
Short date, Numeric date and Long date, with live previews. Choosing a format
returns to the calendar and saves that output's clock preference. Time remains
24-hour; the same choices are available in Change layout.

## Canvas tools

The first button chooses between **Select desktop items**, **Pan canvas**, and
**Search canvas**. Click the selected tool to open its choices. After using
another tool, the first click returns to your last choice; click again to open
the menu. **❖ + V** selects desktop items and **❖ + H** selects Pan canvas.
The letters alone remain ordinary text. Search also finds these tools by name.
**❖ + K** selects Search and opens its field immediately.

The remaining buttons select window groups, open Change layout, and toggle
Overview. On narrow outputs, **Canvas tools** opens one menu with all six choices.
Selection is shared across outputs; each output keeps its own popup.
**❖ + G** selects window groups, **❖ + A** opens Change layout, and **❖ + O**
toggles Overview. The fixed bindings are listed in
[Settings → Keyboard shortcuts](SETTINGS.md#keyboard-shortcuts).

## Change layout

Open **Change layout** from Canvas tools, canvas Search, or Settings → System.
Select a control, desktop item, window, or group, then use its editor to choose
Background, Canvas, or Frame. Background and Canvas move as you pan; Background
is the lower canvas layer. Icons and controls on either remain beneath ordinary
windows. Frame stays attached to an output edge. Wi-Fi, clock, sound, brightness, battery,
application tray and power can be separated from the default system group.
Canvas tools and the app dock remain composite controls.

At widths of 760 logical pixels or less, unchanged default groups share full-width
top and bottom bars. Custom placements remain intact. Canvas tools collapse to a
menu button. Long groups scroll along their edge, and keyboard focus reveals the
focused control without moving the canvas.

Drag toward an edge or one of its corner guides to attach an item. Corner drops
preserve the tool's current horizontal or vertical orientation. Guides and drop
highlights sit beneath frame tools; each tool retains its own selection outline.
Hold Alt to align the along-edge position with the canvas grid. Dropping onto
another frame item forms an ordered group. Use the group's grip to move the
whole strip; select a member to change its options or choose **Separate**.
**Ungroup** releases all members. Groups remain flat, so merging two strips
produces one ordered strip rather than nested groups.
Moving a group onto Canvas or Background releases its members at their displayed
positions. Removing a group hides its controls and returns its windows and desktop
items to the canvas; it does not close applications or delete files. Ordinary
window movement does not attach a window to the frame: placement changes belong
to Change layout.

Pinned windows retain their live application buffers. Their edge strip expands
on hover and opens fully when activated, including with a touchscreen tap.
During layout editing, the resting strip stays stable for selection and movement.
Focusing elsewhere collapses it. Search,
Alt+Tab and dock activation reveal the existing pinned window without panning the
canvas. A close request prompts for confirmation; removing the attachment instead
returns the still-open window to the canvas. Desktop items become compact
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

## Popup ownership

An acknowledged opening survives transient layout-query failures for at most five
seconds. Queries retain their 750 ms deadline and never replay the anchor command.
A shared cancellation ticket covers both pending work and queued UI delivery;
dismissal, replacement, output removal, lock/VT recovery, and layout editing revoke
it. Permanent transport or invalid-snapshot failures cancel it immediately.

## Agent control

Buttons retain descriptive accessible names despite showing icons: `Apps List`,
`Notifications`, `Search canvas`, `Canvas map`, `Select window group`, `Wi-Fi settings`,
`Brightness settings`, `Sound settings`, `Date and time`, and `Power`.
Close controls include `Close apps list`, `Close notifications`,
and `Close power menu`. Dock names include the app, window count, active state,
and next window when cycling.
`Canvas map` toggles Overview open and closed; Escape also closes it.
The power choices are `Lock`, `Log out`, `Confirm log out`, and `Cancel`.
Search rows retain button roles and the names `Open: APP NAME` or `Window:
WINDOW TITLE`; the current result exposes the accessibility `selected` state.

Unlocked `status.shell_corners` describes legacy corner surfaces when present, with
`corner`, `output`, `x`, `y`, `width`, and `height`. The current shell uses independent
frame surfaces instead. Use `layout-status` for their measured bounds, orientation,
visibility and group identity, plus pinned windows, editor selection and animation
state. These bounds use global output-logical coordinates.

`canvas-tool select|pan|search|group|layout` explicitly selects a tool; repeating
the same choice does not toggle it off. `layout-status.canvas_tool` reports the
confirmed choice or `null` when no tool is selected or available. Legacy toggle
commands remain supported.

`frame-clock-format {"output":"eDP-1","value":"long"}` changes an active output's
clock format without opening Change layout. Values are `none`, `weekday`, `short`,
`numeric` and `long`. It uses the normal navigation guards and persists with that
output's layout; invalid outputs or formats are rejected.

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
Setting `HEARTHSPACE_SHELL_EVIDENCE` to a fresh absolute directory
retains its screenshots, status and semantic observations outside temporary
runtime cleanup. The main workflow uses a 2560×1600 output at scale 2. A second
800×600 output at scale 1 checks popup geometry and pixels, storing evidence in
the `small-output` subdirectory. Choose a fresh evidence directory for each attempt.

The same assertions run on real hardware through the existing native harness:

```sh
scripts/shell-smoke --session "$PWD/artifacts/shell-native-1"
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

<a id="layout-editor-crash-2026-10-22"></a>
<a id="layout-editor-redraw-crash-2026-10-23"></a>

## Transient surface lifecycle

Layout editing creates and retires shell surfaces while the UI event loop still
has work queued. The [Winit cleanup patch](../patches/README.md#winit-wayland-destroyed-window-updates)
discards configure updates for destroyed windows. The
[Masonry lifecycle patch](../patches/README.md#masonry-retired-window-presentation)
cancels pending first frames and skips retired targets in deferred redraw work.
Their regressions require a surviving window to keep resizing, drawing, and
receiving input after another surface closes. Preserve these contracts when
updating UI dependencies or changing layout-editor window ownership.
