# Snap applications in a native session

Launch installed Snap applications from **Apps List**, the app dock, or their
desktop shortcuts. This uses Hearthspace's managed launch path, including the
Wayland socket and application scope required for confinement. File opening
through desktop-item Open and Open With uses the same scope preparation.
Launching a Snap command directly in a terminal or through an external portal
does not receive this catalog wrapper.

## Scope and bus ownership

Snap applications require a systemd tracking scope.
Catalog launches create the proper `snap.INSTANCE.APP-UUID.scope` through the
user manager, then restore the application's runtime and selected desktop bus
before executing its original command. Normal native sessions use the standard
user bus; only the explicit private harness requires a separate bus. The
compositor supplies the Snap-accessible Wayland socket. Scope ownership and
confinement remain enforced in both modes.

The explicit private session bus uses a unique Linux abstract Unix socket named
`/tmp/dbus-hearthspace-UUID`. Snap gives applications a separate `/tmp` mount,
which can hide a pathname socket used by a private bus.
An abstract socket remains reachable across those mount
namespaces and is supported by Snap's existing desktop D-Bus policy.
`hearthspace-session-bus` directly executes the system `dbus-daemon` with an
address override; the distribution's session policy, authentication, activation,
and file-descriptor passing remain in effect. `dbus-run-session` still owns and
reaps the daemon. This is the private Hearthspace bus, not the shared user bus.
These are the supported
[`dbus-daemon --address`](https://dbus.freedesktop.org/doc/dbus-daemon.1.html)
and [`dbus-run-session --dbus-daemon`](https://dbus.freedesktop.org/doc/dbus-run-session.1.html)
options. Supplying one address also avoids a multi-listener address list that
Hearthspace's current zbus clients cannot parse.

The native session starts an owned `hearthspace-apps-RUNTIME.service` whose
description records the exact private runtime. Its
`hearthspace-app-scope-guard` process watches the supervisor's PID and Linux
process start time. Each application scope has `BindsTo` and `After`
dependencies on that service. A dead supervisor, removed runtime, or closing
marker ends the guard and its app scopes. PID reuse cannot extend that lifetime.

Normal shutdown marks the session closing before stopping its owner service
and verifying that the owner and its bound scopes have stopped. Native harness
recovery independently derives the owner name from its root-owned session
record, checks the service description, and performs the same bounded cleanup.
It will not report complete cleanup or remove the runtime while an owned scope
is still active. The closing marker also prevents a delayed launch from
successfully restarting the guard after shutdown.

Build/install all binaries from the `hearthspace-session` package:

```sh
cargo build --locked -p hearthspace-session --bins
```

The guard and `hearthspace-session-bus` belong beside `hearthspace-session`,
including when using `--binary-dir`. Ordinary shared sessions require the standard
user bus and systemd user manager.
Private diagnostic sessions without a user manager cannot create managed Snap
scopes. Nested sessions do not create this native scope owner.

## Limits and compatibility checks

- This integration covers catalog launches using `/snap/bin/APP` or
  `snap run APP`, including exported commands with leading `env` assignments.
  Shortcuts without `X-SnapInstanceName`, such as Steam-generated game entries,
  also receive the scope and socket when their executable resolves to
  `/snap/bin/APP`. Resolution respects PATH order and the desktop working
  directory; an ordinary executable that shadows a Snap command stays ordinary.
  Direct Snap commands typed in a terminal and external desktop-entry launches
  by a portal currently bypass the catalog wrapper.
- Snap expects its document portal at `/run/user/UID/doc`, used by normal shared
  sessions. The explicit private harness's portal mount has a different path.
  Confined file selection must pass the
  exact-content checks in [native app acceptance](DAILY_APPS_TESTING.md) before
  it is considered supported; opening a browser window alone is insufficient.

The tracking-name and existing-scope behavior follows Snap's
[tracking implementation](https://github.com/canonical/snapd/blob/2.76/sandbox/cgroup/tracking.go)
and [launcher](https://github.com/canonical/snapd/blob/2.76/cmd/snap/cmd_run.go).
Validation should retain the application scope name, selected bus identity,
visible browser results, and completed native cleanup alongside screenshots.
