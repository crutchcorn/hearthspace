# Applications

Open **Apps List** from the app dock or by tapping Meta, type an app's name, and
press Enter to launch it. Apps List includes installed desktop entries and open
windows. Choosing a window brings that existing window into view. The dock keeps
favorites available when closed and groups running windows by application.
See [shell controls](SHELL.md) for keyboard navigation and
[desktop items](DESKTOP_ITEMS.md) for shortcuts, files, and Open With.

Hearthspace runs native Wayland applications and X11 applications through
XWayland. Applications and toolkits can draw their own title bars or request
compositor decorations. [Window management](WINDOW_MANAGEMENT.md) explains
focus, sizing, fullscreen, and the differences between those title bars.
Application-specific behavior, packaging, portals, and single-instance rules
still affect compatibility; a successful launch does not establish that every
workflow in an app works.

## Application graphics preference

Open an application's context menu in Apps List, the dock, or its desktop
shortcut and select **Run using dedicated graphics card**. Right-click, or
focus the control and press the Context Menu key or Shift+F10. Apps List and
dock controls also support touch and hold. The checkmark changes after saving
succeeds; a failed save leaves the previous preference in place and offers
another attempt.

This preference applies to future launches. It cannot change the GPU used by
an already-running application. Clearing it restores ordinary launch behavior;
it does not force integrated graphics. Applications and drivers ultimately
decide which GPU to use.

Hearthspace saves the standard `PrefersNonDefaultGPU` desktop-entry hint in a
user-local override, preserving the entry's other fields and desktop actions.
Launch preparation asks switcheroo-control for the dedicated GPU's environment
and applies it only to that application's child process. If no suitable GPU
is reported, launch retains its ordinary behavior. Entries that support only
D-Bus activation retain activation; they cannot receive a new process
environment through an existing service.

## Application configuration

Normal sessions inherit the user's application configuration directories. An
editor launched from Apps List and from a terminal therefore uses the same
preferences. When two desktops share profiles, an application's single-instance
behavior may reuse a window on the other desktop. Use that application's own
new-instance or profile option when needed.

Set `HEARTHSPACE_ISOLATE_APP_PROFILES=1` before starting a session to use separate
per-application XDG directories under Hearthspace's state directory. The native
test harness sets this explicitly. Hearthspace Settings always uses the session
daemon's configuration path.

Terminal desktop entries use `xdg-terminal-exec` when available, with terminal
preferences from `xdg-terminals.list` as a fallback. [Snap applications](SNAP_APPLICATIONS.md)
need the session's owned application-scope service for catalog launches; direct
terminal and external portal launches do not use that wrapper. A reachable
Wayland socket alone does not establish confined file or portal access.

## Launch services and environment

The shell queues launcher, dock, gesture, and Settings requests on a bounded
launch task, independent of desktop filesystem work. The compositor shares one
bounded preparation queue for apps and files; filesystem/catalog work and GIO
activation run outside the input loop. It rechecks session, lock, output, and
drawing ownership before accepting a prepared launch. Activation helpers have
deadlines; a successfully spawned application has its own lifetime. Errors are
returned to the requesting UI.

Shell launch requests carry their original ten-second click deadline through the
`launch-request-v1` envelope. The compositor's eight-second preparation limit can
shorten that budget but cannot renew it. Cancelling the UI request closes its
registered connection immediately; preparation checks the caller before creating
a child. Legacy command clients retain their half-close protocol, and internal
fire-and-forget launches do not acquire an artificial caller lifetime.

`hearthspace-desktop` owns desktop-entry parsing, argument expansion, file/app GIO
activation, favorites, and neutral artwork types. `DBusActivatable` entries may
omit `Exec`. Terminal entries and Settings arguments preserve their direct command
semantics, including literal arguments and working directories.

Ordinary applications in shared native sessions use the canonical user runtime
and an absolute path to Hearthspace's Wayland socket, whether launched directly
or through D-Bus. Private and nested sessions retain their isolated connection
policy. Nested supervisors clear inherited activation ownership and selected-bus
metadata before applying their own explicit values. Native GIO activation requires
that selected bus; it cannot silently borrow a parent desktop's activation route.
Ownership records and Hearthspace control sockets remain under
`HEARTHSPACE_SESSION_DIR`; explicit audio overrides remain intact.

Shared activation publication has a generation-owned journal. XWayland readiness
and disconnection update `DISPLAY` through that ownership boundary, off the
compositor loop. Launch synchronizes publication before GIO activation. Retirement
rereads the latest journal and preserves independently changed manager values.
The transient `activation-display.json` record belongs to the verified session
runtime and is removed with it after the process-cleanup barriers.

## GNOME Terminal / Ptyxis

For Ptyxis desktop entries such as `org.gnome.Ptyxis.desktop`, the session supplies
an adapter for the terminal's `systemd-run --user --scope` shell invocation.
This lets the shell reach the user manager even when its application runtime is
private, and keeps its scope owned by the Hearthspace session. Changing the D-Bus
address alone cannot redirect systemd's runtime-directory socket lookup.

Native sessions place the adapter in their application PATH. It recognizes the
Ptyxis scope invocation, connects to the user manager, then restores the
application's runtime and selected D-Bus environment before executing the original
shell command. Ownership records come from `HEARTHSPACE_SESSION_DIR`, independently
of an activated app's canonical runtime. Version queries and other invocation
shapes pass through unchanged. The adapter uses `hearthspace-app-scope-guard`;
it does not replace the system binary.

Each Ptyxis scope is recorded before execution and receives `BindsTo` and `After`
dependencies on the session's application-scope owner. Normal shutdown and the
independent crash guard therefore include disowned terminal jobs, which would
otherwise escape the login cgroup. Native recovery recognizes only the recorded
Snap names and exact `ptyxis-spawn-UUID.scope` names.

The recognized invocation is defined in
[`ptyxis_run.rs`](../crates/hearthspace-session/src/app_scopes/ptyxis_run.rs).
An upstream change to Ptyxis's invocation shape needs a compatibility review;
this is not a general wrapper for arbitrary systemd scopes.

## Steam game shortcuts

A Steam-generated shortcut may contain `Exec=steam steam://rungameid/…` without
`X-SnapInstanceName`. Catalog launch resolves the executable before deciding
whether to supply Snap's scope and Wayland socket. If `steam` resolves to
`/snap/bin/steam`, it receives the Snap launch environment. PATH precedence and
ordinary executables are preserved.

The private-session bus is reachable across Snap's private temporary-file mount;
installed native sessions use the standard user bus; native development sessions
start a private bus. See
[Snap applications](SNAP_APPLICATIONS.md) for ownership and portal limitations.
Game launch, gameplay, controller input, and file access are separate compatibility
checks; Steam opening successfully does not establish the rest.

## Checking application workflows

Desktop Discord and browser screen sharing use the ScreenCast portal and
PipeWire. See [screen and window sharing](SCREEN_SHARING.md) for the backend,
chooser, setup, and request diagnostics.

[Native app acceptance](DAILY_APPS_TESTING.md) describes repeatable browser,
editor, clipboard, and file-selection checks. Use exact saved or transferred
content as well as visible application state when evaluating a workflow.
Keep package-specific limitations with the relevant integration guide rather
than treating an app list as a blanket compatibility guarantee.
