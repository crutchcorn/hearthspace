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

The shell's **Power → Log out** action returns to the login screen after
confirmation.
Choose **Ubuntu** there to return to GNOME. The installer does not change which
desktop is selected by default; GDM manages your saved session selection.
This host already has automatic login enabled. Its configuration is preserved;
logging out allows you to make an explicit session selection.

Super+L and the shell's **Power → Lock** action start the distribution's swaylock.
Its blue indicator remains visible while idle; typing shows password-entry feedback,
keyboard layout, and Caps Lock state. Enter your normal account password and
press Enter. Agents cannot enter or capture passwords or bypass authentication.
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
gnome-session or use its private registration convention. This follows the
[GDM Wayland wrapper](https://gitlab.gnome.org/GNOME/gdm/-/blob/50.1/daemon/gdm-wayland-session.c).

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
Real GDM login and swaylock password unlock passed on the GPD on 2026-10-15;
see the [manual acceptance record](POWER_TESTING.md#real-gdm-login-and-password-unlock--2026-10-15)
for the observed hardware, preserved applications and physical input checks.

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

These earlier harness runs did not establish actual GDM authentication, visible
lock feedback or real password unlock. The subsequent GPD manual run below
covers those checks.

### Application launch freeze (2026-10-21)

On the ThinkPad T480s, opening OpenCode stopped the deployed compositor,
Xwayland, and OpenCode in the same background process group. OpenCode's
`/bin/bash -il -c 'env -0'` helper inherited tty2 as its controlling terminal.
Tracing that helper recorded `TIOCGPGRP` returning the supervisor's foreground
group, followed by `kill(0, SIGTTIN)`. Null standard input did not prevent Bash
from opening `/dev/tty` and stopping the entire compositor group.

Terminating only the stuck environment probe and continuing the affected group
restored the existing desktop; its compositor PID was preserved and a fresh
native page-flip capture succeeded. Evidence is retained under
`/tmp/hearthspace-freeze-20261021/`. The supervisor now uses `setsid()` for managed
children, removing the inherited controlling terminal while preserving group
cleanup. A private-PTY regression exercises the interactive shell probe and
descendant shutdown without touching the real login terminal.

Optimized release `6c96633cb8116` was installed on the ThinkPad; all 12 release
files matched the installation manifest's hashes, modes and root ownership.
Workspace formatting, Clippy with warnings denied, all-target builds and all
1,320 Rust tests passed with `udev,e2e,test-apps`. Ruff lint and formatting passed
all 154 Python files and entrypoints. Headless checks used the documented
software renderers and removed the inherited host D-Bus addresses. The shell's
Waydriver fixture now also isolates XDG directories before starting the shell,
preventing the host application catalog from changing its startup workload.

The complete Python suite ran with E2E, AT-SPI and the tray library required:
405 passed, with seven failures and two errors. Eight shell fixtures still
expect retired corner metadata/accessibility containers or the battery's old
button role. The terminal smoke exhausted its shared 60-second deadline at its
final cursor check, after exact input, rendering, movement, resizing and software
cursor checks passed. These fixtures launch their children directly with Python's
`start_new_session=True`; they do not exercise the changed supervisor path.
These unresolved checks are retained in `python-tests.log`, alongside the full
Rust/build/lint results in the diagnostic directory above.

Installed native verification used the ThinkPad's Intel UHD Graphics 620 / i915,
kernel `7.0.0-38-generic`, no virtualization, and eDP-1 at 1920×1080 / scale 1.
Run `/tmp/hearthspace-freeze-native-20261021-2` passed startup, exact terminal input
and visible output, OpenCode launch and presentation, and continued compositor
responsiveness with fresh page flips after launch. The compositor, shell,
settings daemon and notification daemon each had `PID == PGID == SID` and no
controlling terminal. The harness used the installed binaries in login-session
mode with an isolated profile; this did not repeat GDM authentication.

Both owned runs stopped their session scopes, removed their private runtimes,
reported `cleanup.complete`, and restored the original tty2 with the existing
Ubuntu login and background agent intact. Run `-1` passed terminal input and
process identity assertions but stopped early because the temporary evidence
logger tried to serialize bytes; it does not establish OpenCode acceptance.
Its script/log and the successful repeat's manifests, binary provenance,
PNG/metadata pairs and cleanup reports are retained. The installed fix takes
effect on the next Hearthspace login.

### GDM login regression (2026-10-15)

On the ThinkPad T480s, selecting Hearthspace and authenticating through GDM
reached the installed supervisor, then returned to the greeter after ten seconds.
The journal showed that the cleanup guard rejected the login before the compositor
started. The original check required logind's `Desktop=Hearthspace`, while GDM
50.1 on this host leaves `Desktop` empty, including for ordinary Ubuntu logins.
The GDM-specific selection check described above addresses this failure, with
regressions covering startup, foreign-desktop rejection, process identity races,
and cleanup after the supervisor exits. A successful manual retry on the
ThinkPad T480s remains unverified; the GPD acceptance below is separate hardware.

Corrected release `9dedca6ea14d` was installed; its helper matched the reviewed
source and installation hashes/modes/ownership were verified. Formatting, Clippy,
workspace and release builds, 545 Rust tests, and 311 Python tests passed; eight
opt-in headless Python tests were skipped. Ruff passed all 109 Python files and
entrypoints. The live Ubuntu GDM session was rejected by a read-only ownership
check. Validation logs are `/tmp/hearthspace-gdm-fix-*.log`.

### Real GDM login and password unlock — 2026-10-15

The user selected **Hearthspace** in GDM and entered their password locally on
the GPD. Installed release `44e7eb81dec29`, built from `main` commit `f78b92f`,
started successfully on tty2. The guard and live process identities establish
logind session 22, `Service=gdm-password`, `Seat=seat0` and
`GDMSESSION=hearthspace`; logind's `Desktop` remained empty. The running core
executables matched that installed release. This was the user's actual
graphical login, with agents attached to its explicit private socket.

The same login survived a 45.066-second suspend and local swaylock password
unlock. The user confirmed the blue idle indicator, password-entry feedback,
restored Calculator and terminal, and working physical keyboard, touchpad and
touchscreen. Agents verified preserved core processes and window geometry,
fresh native frames and exact terminal input. The initial unlock observer
timed out before the remote agent could signal the user; a bounded continuation
observed the same original locker authenticate, without another suspend.

The [power acceptance record](POWER_TESTING.md#real-gdm-login-and-password-unlock--2026-10-15)
contains detailed evidence and the retained timeout. The desktop remained active
on tty2 with both background T3 services preserved. This run did not repeat
logout, crash cleanup, external-display or other-machine testing.
