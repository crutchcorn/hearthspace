# App Notes

This is a running compatibility log for apps tested through Hearthspace's app launcher and Wayland compositor.

## Tested Apps

| App | Source / Type | Notes |
| --- | --- | --- |
| Foot | Native Wayland terminal | Keyboard launch/focus, Unicode input, browser clipboard transfers, and close/focus fallback exercised on native hardware. Uses server-side decorations. |
| Terminal (Ptyxis) | Ubuntu native package | Catalog launch, shell input, private session environment, and disowned-job cleanup after normal shutdown and compositor failure verified. See [Ptyxis scope ownership](#gnome-terminal--ptyxis). |
| A11yTest app | Hearthspace test app, GTK4 | Our in-repo accessibility test app. Uses client-side decorations. |
| Micro | Terminal app | Launches through terminal app handling. |
| SmartGit | Desktop app | Earlier testing opened it in GNOME instead of Hearthspace. Needs re-test after launcher environment fixes. |
| Visual Studio Code | DEB install | Native editing and saving exact Unicode bytes verified; semantic actions work alongside the browser accessibility tree. |
| Text Editor | GTK desktop app | Reopened and saved the editor's Unicode file in a native session. |
| Nautilus | GTK file manager | Native folder creation verified. GVfs mount cleanup belongs to session recovery; see the daily acceptance record for shutdown evidence. |
| Firefox | Mozilla native DEB | Native Unicode entry, browser-to-terminal copy, local navigation, and file selection with exact-content verification exercised. See [package setup](../CONTRIBUTING.md#native-firefox-package). |
| Firefox | Snap install | Catalog launch uses a managed Snap scope and a confinement-compatible Wayland socket. The private document-portal path remains a confined file-selection limitation; see [Snap applications](SNAP_APPLICATIONS.md). |
| Pepper Grinder | Steam Snap | Original game shortcut launched Steam and the game; keyboard input reached save selection. Scope ownership and complete process cleanup verified. See [Steam game shortcuts](#steam-game-shortcuts). |

These checks cover specific workflows, not every application feature. Retained
native evidence and outstanding acceptance are recorded in
[daily usability](../todos/DAILY_USABILITY.md); repeatable browser/editor/file
checks are described in [native app acceptance](DAILY_APPS_TESTING.md).

## Notes

Normal sessions inherit the user's application configuration directories. An
editor launched from the app search and from a terminal therefore uses the same
preferences. Set `HEARTHSPACE_ISOLATE_APP_PROFILES=1` before starting a session to
use separate per-application XDG directories under Hearthspace's state directory.
The native test harness sets this explicitly so test launches can coexist with
applications on the original desktop. Hearthspace Settings always uses the
session daemon's configuration path.

Application single-instance behavior still belongs to the application. When
running two desktops with shared profiles, use the application's own new-instance
or profile option if it would otherwise reuse a window on the other desktop.

- Server-side decorations are negotiated by the app/toolkit and drawn by Hearthspace when applicable.
- Client-side decorations are drawn by the app/toolkit itself.
- Snap catalog launches require the session's owned application-scope service;
  direct terminal or portal launches do not use that wrapper. Socket compatibility
  alone does not establish confined portal/file access.
- Terminal apps should go through `xdg-terminal-exec` when available, with `xdg-terminals.list` fallback behavior.

## GNOME Terminal / Ptyxis

Ubuntu 26.04's **Terminal** entry is `org.gnome.Ptyxis.desktop` (Ptyxis 50.1).
Ptyxis starts each shell through `systemd-run --user --scope`; systemd looks for
its manager at `$XDG_RUNTIME_DIR/systemd/private`. Hearthspace's private runtime
does not contain that host socket, which caused **Failed to connect to user scope
bus via local transport**. Changing the desktop D-Bus address cannot fix that
direct connection.

Native sessions place a small `systemd-run` adapter in their private application
PATH. It recognizes Ptyxis's scope invocation, connects to the host manager, then
restores the application's runtime and selected D-Bus environment before
executing the original shell command. Ownership records come from
`HEARTHSPACE_SESSION_DIR`, independently of an activated app's canonical runtime.
Version queries and other invocation shapes pass through unchanged.
The adapter uses the existing `hearthspace-app-scope-guard` executable; it does not
replace the system binary. Shared session startup separately publishes the
activation environment; private harness sessions never modify the user manager.

Each Ptyxis scope is recorded before execution and receives `BindsTo` and `After`
dependencies on the session's application-scope owner. Normal shutdown and the
independent crash guard therefore include disowned terminal jobs, which would
otherwise escape the login cgroup. Native recovery recognizes only the recorded
Snap names and exact `ptyxis-spawn-UUID.scope` names.

The compatibility boundary follows
[Ptyxis's scope launcher](https://gitlab.gnome.org/chergert/ptyxis/-/blob/50.1/agent/ptyxis-run-context.c#L1077-L1151)
and [systemd's direct user-manager connection](https://github.com/systemd/systemd/blob/v259/src/shared/bus-util.c#L268-L305).
Revisit the adapter when Ptyxis provides a supported way to supply scope ownership
or gracefully runs without a reachable manager.

Native acceptance on 2026-10-21 passed with installed release `ac80062020a0` on
the physical ThinkPad T480s, Intel UHD 620 (`i915`), kernel `7.0.0-38-generic`,
and eDP-1 at 1920×1080, scale 1. Both runs launched the catalog entry and typed
`HEARTHSPACE_PTYXIS_OK`, checked the exact file contents and visible terminal
output, and verified the shell's private runtime and D-Bus address. The shell
and its disowned background job belonged to the recorded scope, with `BindsTo`
and `After` dependencies on the correct session owner. Both normal shutdown and
an injected compositor crash stopped that job and removed its scope. Cleanup
restored tty2, stopped the owned services, and removed their runtimes; the
original GNOME desktop and background agent remained active.

Evidence is retained in `/tmp/hearthspace-ptyxis-normal-20261021-1` and
`/tmp/hearthspace-ptyxis-crash-20261021-1`, including page-flip captures,
scope properties, typed markers, process cgroups, and cleanup manifests.
Separate manager integration checks also verified owner-stop and supervisor-death
cleanup. Full checks under `/tmp/hearthspace-terminal-scope-20261021` passed
all 1,331 Rust tests, rustfmt, Clippy with warnings denied, the workspace build,
and Ruff lint/format. The full Python suite passed 410 of 419 tests, including
all five added cleanup regressions; the same nine failures as the preceding run
remain in eight outdated shell fixtures and the terminal smoke's cumulative
deadline at its final cursor check. Native acceptance ran separately from the
headless GPU suites.

Both normal and crash cases passed again with the private abstract D-Bus socket
in release `29e6b4bb6a1d`. Evidence is retained in
`/tmp/hearthspace-ptyxis-normal-20261021-2` and
`/tmp/hearthspace-ptyxis-crash-20261021-2`; exact shell input, private environment,
disowned-job cleanup, runtime removal, and VT restoration all passed.

## Steam Game Shortcuts

Steam generated `Pepper Grinder.desktop` with
`Exec=steam steam://rungameid/2076580` and no `X-SnapInstanceName`. On this host,
`steam` resolves to `/snap/bin/steam`. The missing metadata caused Hearthspace
to skip Snap's tracking scope, so Snap refused to start the application. Catalog
launch now resolves the executable before deciding whether to provide Snap's
scope and Wayland socket. PATH precedence and ordinary executables are preserved.

Once Steam could start in the private harness, Snap's private `/tmp` hid that
desktop's D-Bus socket. Explicit private sessions use the abstract socket
described in [Snap applications](SNAP_APPLICATIONS.md), preserving the normal
daemon policy and lifetime. Normal native sessions now use the standard user bus.

Native acceptance on 2026-10-21 passed with installed release `29e6b4bb6a1d`
on the physical ThinkPad T480s, Intel UHD 620 (`i915`), kernel
`7.0.0-38-generic`, eDP-1 at 1920×1080, scale 1. The isolated catalog received
a byte-identical copy of the user's original game shortcut. Launching that entry
opened Steam and an Xwayland `PepperGrinder` window. Enter dismissed the safety
screen, then advanced from the title to the existing save-selection menu.
This establishes launch and keyboard response; gameplay and controllers were
not exercised. The recorded Steam scope was bound to the session owner.
Shutdown stopped all 24 observed Steam/game processes, removed the scope and
runtime, and restored tty2 while the original GNOME desktop and agent continued.

Evidence is in `/tmp/hearthspace-pepper-native-20261021-3`, with screenshots,
input observations, process identities, scope properties, and cleanup results.
Attempt `-1` failed before app launch because the test fixture precreated an
isolated-profile directory. Attempt `-2` started Steam but reproduced the hidden
D-Bus socket. Both attempts cleaned up completely; their evidence is retained.
Checks under `/tmp/hearthspace-pepper-launch-20261021/bus-fix` passed all 1,343
Rust tests, rustfmt, Clippy with warnings denied, the workspace build, and Ruff
lint/format. Default Python checks passed 407 tests with 14 E2E skips. The full
E2E-enabled Python run earlier in this fix passed 410 of 419, with the same nine
failures recorded above. Native checks ran separately from GPU test suites.
