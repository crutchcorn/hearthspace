# Snap applications in a private native session

Snap requires each confined application to run in a systemd tracking scope.
Hearthspace owns a private desktop D-Bus, so Snap cannot contact the shared user
manager through that bus. Catalog launches create the proper
`snap.INSTANCE.APP-UUID.scope` through the host user manager, then restore the
private runtime and desktop bus before executing the original Snap command.
The compositor still supplies the Snap-accessible Wayland socket. Neither the
host manager's environment nor Snap's confinement policy is changed.

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

Build/install both binaries from the `hearthspace-session` package:

```sh
cargo build --locked -p hearthspace-session --bins
```

The guard belongs beside `hearthspace-session`, including when using
`--binary-dir`. A native host without a user systemd manager can run ordinary
applications, but catalog Snap launches report that managed scopes are
unavailable. Nested sessions do not create this native scope owner.

Current acceptance limits remain explicit:

- This integration covers catalog launches using `/snap/bin/APP` or
  `snap run APP`, including exported commands with leading `env` assignments.
  Direct Snap commands typed in a terminal and external desktop-entry launches
  by a portal currently bypass the catalog wrapper.
- Snap expects its document portal at `/run/user/UID/doc`. Hearthspace's private
  portal mount has a different path. Confined file selection must pass the
  exact-content checks in [native app acceptance](DAILY_APPS_TESTING.md) before
  it is considered supported; opening a browser window alone is insufficient.

The tracking-name and existing-scope behavior follows Snap's
[tracking implementation](https://github.com/canonical/snapd/blob/2.76/sandbox/cgroup/tracking.go)
and [launcher](https://github.com/canonical/snapd/blob/2.76/cmd/snap/cmd_run.go).
Validation should retain the application scope name, private bus identity,
visible browser results, and completed native cleanup alongside screenshots.
