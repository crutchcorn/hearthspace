# Graphical login

Install after [setting up dependencies](../CONTRIBUTING.md):

```sh
scripts/install-session install
```

The installer builds an optimized native desktop as the current user, then uses
sudo to install a complete release. Starting an installed session requires neither
Cargo nor the source checkout. Re-run the command after code updates; a running
session remains on its original release until logout. To remove the installation,
log out of Hearthspace first, then run `scripts/install-session uninstall` from
another desktop. User settings and application data are preserved.

## Use from GDM

1. Save your work and log out of the current desktop.
2. Select your user at the login screen.
3. Open the session chooser (the gear button) and select **Hearthspace**.
4. Enter your password normally on the laptop.

The shell's **Log out** action returns to the login screen after confirmation.
Choose **Ubuntu** there to return to GNOME. The installer does not change which
desktop is selected by default; GDM manages your saved session selection.
This host already has automatic login enabled. Its configuration is preserved;
logging out allows you to make an explicit session selection.

Super+L and the shell's **Lock** action start the distribution's swaylock. Its
blue indicator remains visible while idle; typing shows password-entry feedback,
keyboard layout, and Caps Lock state. Enter your normal account password and
press Enter. Agents cannot enter or capture passwords or bypass authentication.
See [session locking](SESSION_LOCK.md) and [power handling](POWER.md).

## Startup, shutdown, and diagnostics

The installed launcher selects the native backend explicitly, creates a private
desktop D-Bus/runtime through the existing supervisor, and uses the login's
logind seat. It leaves the shared systemd user-manager environment untouched.
Startup waits for the shell to map and for fresh native presentation on every
connected output, within the configured startup deadline. Failed startup cleans
up its children and returns control to GDM.

The entry sets `X-GDM-SessionRegisters=false`: GDM's Wayland wrapper performs its
normal session/display registration. Hearthspace does not impersonate
gnome-session or use its private registration convention. This follows the
[GDM Wayland wrapper](https://gitlab.gnome.org/GNOME/gdm/-/blob/50.1/daemon/gdm-wayland-session.c).

The `--login-session` supervisor mode owns an entire Hearthspace login. Its
separate cleanup guard tracks exact process and logind identities; when the
supervisor ends, the guard terminates only that login scope, including detached
descendants. It then removes the verified private runtime, safely detaching known
portal mounts. Snap app scopes also retain their existing supervisor guard.
The [logind session interface](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html)
provides the owning user's session termination. This mode is for the installed
display-manager entry, not for launching a second desktop inside GNOME.

Logs go to the display manager's session journal. Useful read-only commands are:

```sh
journalctl -b -g 'Hearthspace|hearthspace'
journalctl --user -b -u 'hearthspace-login-*.service'
loginctl list-sessions
```

The private runtime is `/run/user/UID/hearthspace/session-PID/`; while running it
contains `processes.json`, `power.json`, sockets and `login-guard-ready.json`.
If login fails, choose Ubuntu and inspect the journal. No display-manager restart
or global policy change is needed to install or remove the session entry.

## Acceptance

Installer regressions stage files without sudo and check completeness, ownership,
release pinning, updates, rollback and uninstall. Supervisor regressions cover
startup deadlines, desktop presentation and failed-bootstrap cleanup. Native
testing can exercise the installed files and login cleanup using the owned VT
harness:

```sh
scripts/hearthspace-native start --session /tmp/hearthspace-installed-1 \
  --no-build --binary-dir /usr/local/lib/hearthspace/current/bin \
  --login-session --watchdog 300
# Inspect, exercise, and always stop this exact owned session:
scripts/hearthspace-agent --session /tmp/hearthspace-installed-1 status
scripts/hearthspace-agent --session /tmp/hearthspace-installed-1 stop
```

This validates the installed native session under the harness's PAM session;
it does not substitute for selecting Hearthspace and authenticating through GDM.
The user has deferred that local check. Real swaylock password unlock is also
pending; the [manual unlock acceptance](POWER_TESTING.md) performs a bounded
user-assisted check without synthetic password input or weakened locking.

### Development-host results (2026-10-14)

The complete optimized release `389778071c3f` was built through the installer
and installed under `/usr/local`; installed file hashes, modes and ownership
were verified. The host's GDM 50.1 library lists `hearthspace` alongside `ubuntu`
and resolves its name to **Hearthspace**. No GDM restart or existing-session
logout was performed.

Native checks used the GPD's AMD Strix Radeon 880M/890M with `amdgpu`, kernel
`7.0.0-38-generic`, no virtualization, and eDP-1 at 2560×1600 / scale 2:

- `/tmp/hearthspace-installed-login-2`: installed startup, native page flips,
  Settings window, exact terminal input, and the shell's **Log out → Confirm
  log out** flow passed. The independent guard reported `login-cleaned`.
- `/tmp/hearthspace-installed-login-6`: final release startup and app discovery
  passed (Settings present, development Storybook absent). Exact input was
  received and visibly rendered in Foot. The real swaylock confirmed protected
  presentation with the new indicator options; capture returned `session is
  locked`. No password was supplied or locked screenshot taken.
- The final run froze its own D-Bus wrapper before sending SIGKILL to its verified
  supervisor. The login guard ended the owned scope, including a detached test
  child, and cleanup completed in approximately 0.41 seconds. The wrapper could
  not initiate its normal exit path while stopped. The guard journal and
  `crash-result.json` record the outcome.
- Every owned run restored the original tty2, stopped its scopes, and removed
  its private runtime. GNOME and the T3/background connection remained available.
  Manifests, binary provenance, guard identity/journal, PNG/metadata pairs, input
  tokens and cleanup reports remain in the artifact directories.

Earlier attempts remain available: login-1 caught a `loginctl` property-query
bug, now fixed with a regression; login-2 exposed a stale compositor build, fixed
by rebuilding the complete release. Login-3 through login-5 stopped safely after
crash-fixture assertions were corrected for `setsid` forking, swaylock's PAM child,
and asynchronous SIGSTOP delivery. These attempts did not establish crash
recovery; login-6 did. The diagnostic run is retained separately.

Final verification passed workspace formatting, Clippy with warnings denied,
all-target builds, 452 Rust tests, and 240 Python tests with E2E and AT-SPI
required (no skips). Ruff lint/format passed all 90 Python files and entrypoints.
Logs are `/tmp/hearthspace-login-complete-{fmt,clippy,build,rust,python,ruff}.log`.
An earlier extra run with `LIBGL_ALWAYS_SOFTWARE=1` failed one headless clipboard
case; its log is retained at `/tmp/hearthspace-login-final-python.log`. The normal
host renderer passed the subsequent full suites; the renderer-specific failure's
cause has not been established.

Actual GDM authentication, visible lock feedback observed on the laptop, and real
password unlock after wake remain pending the user's local testing.
