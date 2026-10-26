# Graphical login

Install the graphical session after [setting up dependencies](DEPENDENCIES.md).
Run the installer from the repository root as your normal user:

```sh
scripts/install-session install
```

The installer builds an optimized native desktop as the current user, then uses
sudo to install a complete release. Starting an installed session requires neither
Cargo nor the source checkout. Re-run the command after code updates; a running
session remains on its original release until logout. To remove the installation,
log out of Hearthspace first, then run `scripts/install-session uninstall` from
another desktop. User settings and application data are preserved.

Each release includes `hearthspace-lock`. The installer also creates the fixed
`/etc/pam.d/hearthspace-lock` policy when absent, owned by root:root with mode
`0644`, and records it for rollback and verified uninstall. Its authentication
and account rules include the existing `login` stacks. An existing secure
administrator-managed policy is preserved and stays outside installer ownership;
modified managed policies stop updates and uninstall for administrator review.
The login stack and GDM's PAM configuration are unchanged.

## Use from GDM

1. Save your work and log out of the current desktop.
2. Select your user at the login screen.
3. Open the session chooser (the gear button) and select **Hearthspace**.
4. Enter your password locally at the login screen.

The shell's **Power → Log out** action returns to the login screen after
confirmation. Choose another installed desktop there to return to it. The
installer does not change which desktop is selected by default; GDM manages your
saved session selection.
If automatic login is enabled, log out to make an explicit session selection.
The installer preserves the display manager's automatic-login configuration.

Super+L and the shell's **Power → Lock screen** action start the release's
`hearthspace-lock`.
The lock screen shows the clock, your name, and a password entry over the desktop
background. Enter your normal account password and press Enter or activate
**Unlock**. Agents cannot enter or capture passwords or bypass authentication.
See [session locking](SESSION_LOCK.md) and [power handling](POWER.md).

## Startup, shutdown, and diagnostics

The installed launcher selects the native backend explicitly, creates a private
Hearthspace runtime, and uses the login's logind seat and shared user bus. It
publishes the ordinary-application environment with generation-owned restoration.
Explicit private harness sessions retain their private bus and never publish into
the shared user manager. See [launch environments](APPS.md#launch-services-and-environment).
Startup waits for the shell to map and for fresh native presentation on every
connected output, within the configured startup deadline. Failed startup cleans
up its children and returns control to GDM.

Managed desktop processes start in separate POSIX sessions without the login's
controlling terminal. This prevents an application's interactive shell helper
from stopping the compositor with a terminal job-control signal such as
`SIGTTIN`. Each managed process remains the leader of its process group, so
supervisor shutdown still signals its descendants together. POSIX sessions are
separate from logind sessions: the desktop keeps its existing login scope and
seat permissions.

The entry sets `X-GDM-SessionRegisters=false`: GDM's Wayland wrapper performs its
normal session/display registration. Hearthspace does not impersonate
gnome-session or use its private registration convention. The entry is shipped in
[`data/wayland-sessions/hearthspace.desktop`](../data/wayland-sessions/hearthspace.desktop).

The `--login-session` supervisor mode owns an entire Hearthspace login. Its
separate cleanup guard tracks exact process and logind identities; when the
supervisor ends, the guard terminates only that login scope, including detached
descendants. Before removing the verified runtime or detaching portal mounts, it
also waits for the owned user-manager application scopes to stop. Normal shutdown
and crash recovery use the same persisted-intent barrier, including partial
startup without an in-memory scope owner. Failed cleanup retains runtime and
recovery records for a later attempt. Runtime identity and mounts are rechecked
after waiting. Snap and Ptyxis app scopes retain their independent supervisor
guard; see [terminal scope integration](APPS.md#gnome-terminal--ptyxis).
GDM can leave logind's `Desktop` property empty because it opens PAM before
exporting the selected desktop. In that case, the guard additionally requires a
`gdm-password` or `gdm-autologin` session and the supervisor's original
`GDMSESSION=hearthspace` launch environment. It records the desktop/service
metadata and requires it to remain unchanged during cleanup; another desktop's
selection is rejected even if Hearthspace has set its own `XDG_SESSION_DESKTOP`.
The guard uses logind to terminate the owning user's verified session. This mode
is for the installed display-manager entry, not for launching a second desktop
inside GNOME.

Logs go to the display manager's session journal. Useful read-only commands are:

```sh
journalctl -b -g 'Hearthspace|hearthspace'
journalctl --user -b -u 'hearthspace-login-*.service'
loginctl list-sessions
```

The private runtime is `/run/user/UID/hearthspace/session-PID/`; while running it
contains `processes.json`, `power.json`, sockets and `login-guard-ready.json`.
If login fails, select another installed desktop and inspect the journal. No
display-manager restart or global policy change is needed to install or remove
the session entry.

An empty inherited `HEARTHSPACE_HOST_XDG_RUNTIME_DIR` is treated as unset, so
the next login uses its valid `XDG_RUNTIME_DIR`. D-Bus activation cannot remove
retired environment keys and can leave this empty value behind. A nonempty host
override still takes precedence and must be an absolute path. The installed
entry's default runtime allocation is covered by real-supervisor regressions,
without the explicit `--session-dir` supplied by the native test harness.

## Acceptance

Installer regressions stage files without sudo and check completeness, ownership,
release pinning, updates, rollback and uninstall. Supervisor regressions cover
startup deadlines, desktop presentation and failed-bootstrap cleanup. These are
separate from an actual graphical login and local password authentication.

To exercise the installed files and login cleanup through the owned VT harness,
follow [native testing](NATIVE_TESTING.md) and use a fresh artifact directory:

```sh
login_run="$(mktemp -d -t hearthspace-installed.XXXXXX)"
scripts/hearthspace-native start --session "$login_run" \
  --no-build --binary-dir /usr/local/lib/hearthspace/current/bin \
  --login-session --watchdog 300
scripts/hearthspace-agent --session "$login_run" status
# Always stop this exact owned session, including after an assertion fails.
scripts/hearthspace-agent --session "$login_run" stop
```

The binary path above is the default installation prefix. Adjust it if the
installation uses a different prefix. This run uses the harness's PAM session;
it does not establish that GDM selection and authentication work.

For graphical-login acceptance, a local user selects Hearthspace in GDM and
enters their password there. Verify the resulting login identity, installed
release, shell presentation, application launch and logout back to the greeter.
An observer may attach to the explicitly identified session but does not own its
lifecycle. Do not use harness shutdown against the user's ordinary login.

For lock and password-unlock acceptance in an owned test session, use
`scripts/power-smoke --locker hearthspace-lock --manual-unlock` with the other
required [power-test options](POWER_TESTING.md). The user authenticates locally;
the harness observes the original locker and compositor while capture and
synthetic input remain denied. A protocol lock fixture or an older locker result
does not establish authentication coverage for `hearthspace-lock`.

Record the release, assertions and cleanup outcome with each run. Environment
coverage belongs in [tested environments](TESTED_ENVIRONMENTS.md); a successful
harness run does not establish another machine's login or suspend behavior.

## Troubleshooting

- **Login returns to the greeter:** inspect the session journal for the first
  startup or guard error. Check that the installed release is complete and that
  the guard accepted the actual login identity. An empty logind `Desktop` value
  alone is not a failure when the verified GDM selection is Hearthspace.
- **An application launch stops responding:** inspect the supervisor and client
  logs and their process states. Managed children should have separate POSIX
  sessions and no controlling terminal. This prevents interactive shell probes
  from sending terminal job-control signals to the compositor's group. Avoid
  process-name-wide kill or continue commands; they cannot establish ownership.
- **A runtime remains after logout or failure:** inspect the guard and application
  scope cleanup records before removing anything. A live scope, changed runtime
  identity or unexpected mount deliberately blocks removal. Restarting the display
  manager or deleting all user runtimes is not a routine recovery step.
- **An update appears unchanged:** an existing login stays on its original release.
  Save work, log out normally and select Hearthspace again.
