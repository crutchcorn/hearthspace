# GTK touchscreen and titlebar acceptance

Use these checks to verify client-side decorations (CSD): titlebars drawn by the
application itself. The workflows exercise real GTK clients, compositor input
routing and native presentation. They complement [daily smoke](DAILY_SMOKE.md)
and [Firefox acceptance](DAILY_APPS_TESTING.md).

Follow the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and [native testing guide](NATIVE_TESTING.md). Build before taking the VT, and run
separately from other native or GPU-intensive suites. Contacts come from owned
uinput devices through libinput; physical finger feel and multi-output behavior
need separate acceptance.

## Prerequisites

Install GNOME Calculator, Python GTK 3/PyGObject and the uinput/libinput dependencies
from [dependency setup](DEPENDENCIES.md). The titlebar matrix additionally needs
Python GTK 4 support and the built `hearthspace-window-test` binary. Both workflows
require one active output with at least 960×800 logical pixels.

Each client uses the owned Wayland display and private session bus, fresh XDG
configuration/data/cache/state directories and memory-only GSettings. Logs and
isolated app state remain with the run artifacts after client cleanup.

## Touchscreen movement

```sh
gtk_run="$(mktemp -d -t hearthspace-gtk-touch.XXXXXX)"
scripts/gtk-touch-smoke --session "$gtk_run"
```

The default workflow starts and owns its native session. It checks GNOME
Calculator (GTK 4) and a GTK 3 HeaderBar fixture at canvas scales 1.0 and 0.8:

- Two movement checkpoints while a contact stays held, followed by release and
  another drag.
- Rendered client content moving with the window and vacated pixels repainting.
- Pointer movement after finger lift, with no continued window drag.
- Content taps computing `2 + 3 = 5` in Calculator and incrementing the GTK 3
  counter, confirmed through private AT-SPI and captures.

An owned dark terminal backdrop gives light GTK content enough contrast for the
pixel checks. The fixture requires coherent translated content and repainting;
window geometry alone cannot pass. `--case calculator` or `--case gtk3` selects
one client. `--inspect-only` captures initial windows and accessibility trees
without injecting input; inspection omits the backdrop and movement assertions.

The workflow attempts client, input-device and native-session cleanup on failure.
Review the cleanup result as well as the original assertion. Do not start another
native run while the previous run still owns a session or device.

## CSD maximize and titlebar gestures

This mode **attaches to an existing owned native session**. Its caller owns VT
startup, the watchdog and final shutdown. For example, after building the binaries:

```sh
titlebar_run="$(mktemp -d -t hearthspace-gtk-titlebar.XXXXXX)"
scripts/hearthspace-native start --session "$titlebar_run" --no-build --watchdog 1500
scripts/gtk-touch-smoke --session "$titlebar_run" --titlebar-maximize \
  --window-test "$PWD/target/debug/hearthspace-window-test" \
  --timeout 450 --total-timeout 1200
# Run even if the acceptance command fails:
scripts/hearthspace-native stop --session "$titlebar_run"
```

GTK 3/4 HeaderBar fixtures distinguish header gestures from maximize buttons.
Pointer double-clicks and touchscreen double-taps must canvas-maximize and
restore. Maximize buttons must enter focused view while retaining the application
header and hiding the canvas frame. An explicit header gesture leaves focused
view for canvas maximization. F9 in `hearthspace-window-test` exercises an ordinary
maximize request without the GTK-specific header gesture.

The matrix also checks Super+M, Super+Shift+F and application F11 as separate
actions. Each transition verifies committed client state, rendered input and
fullscreen signals at canvas scales 1.0 and 0.8. Restore must recover the original
floating geometry. Focused entry checks each live shell widget before deliberate
hover reveal and after hiding it again.

The current matrix has 44 transitions per GTK toolkit and 20 for the generic
fixture. `--case gtk3`, `--case gtk4` or `--case generic` selects one. Allow 450
seconds per application and 1,200 seconds overall; the owning session's watchdog
must also cover startup and inspection. These budgets are separate from the
compositor's IPC deadlines. Each invocation keeps its evidence in a new
`gtk-titlebar-*` artifact subdirectory and stops its clients, but never stops the
borrowed native session.

This covers cooperating GTK headers and ordinary maximize requests. Custom
application titlebars need their own checks. See the
[GTK protocol contract](../crates/hearthspace-compositor/protocols/README.md)
for request validation and compatibility limits.

## Diagnosing a failure

Preserve client logs, Wayland traces where enabled, libinput traces, accessibility
snapshots, window geometry and PNG/page-flip metadata. Record the toolkit versions,
output scale and tested build with those artifacts; environment coverage belongs
in [tested environments](TESTED_ENVIRONMENTS.md).

A GTK client can send a redundant `unset_fullscreen` immediately before a titlebar
move request. That request must not cancel a valid held touch sequence or replace
pending geometry. Move/resize authority still requires a valid seat, serial and
target window. A transient dialog's drag must survive unrelated presentation
requests from its parent. [CSD regressions](NATIVE_TESTING.md#csd-titlebar-regression-coverage)
cover those boundaries alongside native acceptance.

Use fresh accessible bounds after state changes. GTK shadow margins can offset
the window's reported origin, and GTK 4 can retain the accessible label
**Maximize** while maximized. The fixture derives content coordinates from its
HeaderBar and checks committed state instead of relying on the label alone.
For pixel comparisons, one coherent translation must explain the client samples;
the rounding allowance is one output-logical pixel. A stationary image, stale
copy or failure to repaint vacated pixels remains a failure.
