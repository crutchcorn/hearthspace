# Daily usability

## Goal

Make Hearthspace practical for a sustained local work session on this machine's
native TTY: launch and use a browser, editor, terminal, and file manager; find and
manage their windows without losing work; retain ordinary application settings;
and perform the same work through documented agent commands. Session startup,
shutdown, and failures must preserve the background agent and restore the prior
desktop. Completion requires observed workflows, not just available commands.

This builds on [AI enablement](./AI_ENABLEMENT.md), the
[native backend](./NATIVE_BACKEND.md), and the [roadmap](./ROADMAP.md). The planning
review also covered cursor behavior, live window metadata, accessibility, and
window activity. Destructive idle reclamation and speculative AI features do not
help this milestone; retain application data and prioritize reliable interaction.

## Acceptance

- [x] Keyboard launch, forward/reverse window switching, close, canvas navigation,
      and recovery of off-screen windows work with overlapping everyday apps.
- [x] Newly mapped apps receive focus; activation matches actual focus; closing a
      focused app returns focus to a useful surviving window.
- [x] Maximize/fullscreen and restore work on Wayland and XWayland, including a
      zoomed canvas; dialogs and menus remain reachable.
- [x] Agents discover installed apps and live windows, target stable window IDs,
      enter Unicode text, and receive errors for failed actions. Screenshots and
      semantic state verify each action's intended result.
- [x] Settings/control clients have bounded I/O. A stalled peer cannot indefinitely
      freeze input, display updates, or subsequent automation.
- [x] A native session has working audio, ordinary application configuration,
      useful desktop service integration, and a clear way to exit safely.
- [x] Frequently needed system interactions (notifications, files, links,
      permissions, and session protection) have working, documented paths.
- [x] Useful spatial state survives a normal restart without automatically
      reopening or destroying application work.
- [x] A repeatable native daily-work smoke test covers the supported workflows,
      repeated captures and input, and complete cleanup. Full format, lint, test,
      and build checks pass.

## Implementation order

1. Fix focus/navigation and truthful agent actions; bound settings transport and
   restore access to host audio services from the private runtime.
2. Add standard window states, reachable dialogs, and a running-window selector.
   Extend semantic agent access and Unicode clipboard operations.
3. Complete session and desktop service integration, persistence, and operational
   documentation. Use existing Smithay and desktop protocol implementations.
4. Exercise everyday apps on the actual display; fix observed failures and retain
   screenshots, metadata, logs, and recovery evidence. Reconcile older plans with
   tested capabilities.

## Baseline findings

The native harness, three-finger zoom, clipboard bridge, and private AT-SPI
access already exist. The audit found no compositor keyboard shortcuts, no
window switching/recovery, missing standard window states, incorrect activation
state, false-success launch responses, unbounded settings I/O, and private
runtime audio sockets missing from session configuration. These are concrete
daily-use blockers rather than a reason to implement every optional protocol.

An intermittent native capture timeout remains unresolved in
`/tmp/hearthspace-three-finger-native-1`; isolated retries passed unchanged.
Bounded settings I/O removes one independently identified class of stalls, but
does not establish the cause of that earlier failure.

## Evidence and limits

Record incremental commits, checks, native artifacts, and unresolved issues here
as work progresses. Hardware claims apply to the actual tested GPU and outputs;
unavailable hardware matrix cases remain in the native backend plan.

### 2026-10-14 foundation and desktop integration

Implemented bounded settings I/O and audio endpoints (`7442bd7`, `ce575b5`),
focus/navigation and Unicode/primary clipboard control (`e67790b`), private
session D-Bus and owned portal/authentication helpers (`7edd151`, `f6d079f`),
keyboard search and running-window selection (`edb08a7`), clock and confirmed
logout (`21ec423`), notification delivery/history/actions (`dbd4002`, `19bc0df`),
standard window states and spatial persistence (`ab488bd`), and semantic agent
commands with numeric accessibility-role fallback (`df1de83`).

The first native pass, `/tmp/hearthspace-daily-foundation-native-1`, used the
physical AMD GPD display on tty7 and restored tty2 with complete owned-process
and runtime cleanup; both T3 background services remained active. It verified
keyboard terminal launch/focus, exact `Café 日本語 🦀` paste, host PulseAudio and
PipeWire access from the private runtime, off-canvas window recovery, semantic
shell activation, twelve increasing page-flip capture IDs, and healthy settings
queries while another client left an unfinished request. Evidence includes
`foundation-results.json`, captures, and terminal output in that directory.

That pass exposed launcher typing focus and missing accessible role names; both
now have targeted regression coverage. Its later clipboard attempt failed after
an earlier zoom action left the fixture layout at 1.25 scale. The workflow now
establishes normal zoom and raises each target before editing; the original
failure remains preserved. A native rerun is still required.

The complete Rust suite with real headless clients and required AT-SPI passed
(`/tmp/hearthspace-daily-phase2-full-tests-2.log`), as did all 95 Python tests with
E2E enabled (`/tmp/hearthspace-daily-phase2-python-e2e.log`). New real-client
regressions cover Wayland/X11 maximize/fullscreen/restore at non-unit zoom,
reachable dialogs and constrained popups, retired popup grabs, and a graceful
restart that restores positions without relaunching clients. Full workspace
Clippy passed with warnings denied. These checks precede session-lock integration.

Remaining acceptance work includes the combined native desktop-service/UI pass,
everyday browser/editor/file workflows, fail-closed session locking, repeated
native daily smoke, and final checks/reconciliation. No daily-ready claim yet.

### Native application and lock passes

`/tmp/hearthspace-daily-native-2` verified keyboard terminal launch, exact Unicode
paste, and a matching running-window search row, but Enter did not refocus the
target. Cleanup completed and restored tty2. A focused real Xilem keyboard/view
regression passes; `/tmp/hearthspace-daily-native-debug-3` subsequently delivered
Enter to the same stable target ID on hardware. The original intermittent failure
is retained; repeated daily smoke is still required. Catalog polling now retains
last successful results across transient I/O failures, and failed submissions
retain post-action screenshots and semantic state.

`/tmp/hearthspace-daily-apps-native-1` exposed failures in real desktop services:
PAM's inherited `CAP_WAKE_ALARM` made Bubblewrap reject GTK image loaders, Snap
Firefox could not establish its required user scope on the private bus, and the
GNOME polkit agent used an X11-only call on a Wayland window. VS Code launched,
but its editing/saving workflow has not yet been verified. All owned processes
and probe profiles were cleaned up; the original tty2 and both T3 services remained
available.

Commit `0f715bc` drops inherited capabilities before the session bus and child
processes. On hardware, the previously failing image-loader probe then produced
the expected 128×128 image, all four capability sets were zero in the supervisor
and helpers, and the GTK portal chooser successfully returned the selected file
URI. The authentication agent is being changed to the distribution's Wayland-aware
MATE agent. Ubuntu's confined `notify-send` also denied `GetCapabilities` and
notification response signals; the narrow local AppArmor extension and installed
packages are documented in CONTRIBUTING.md.

Commit `58b6395` adds `ext-session-lock-v1` with protected presentation barriers,
input isolation, and fail-closed locker loss. The native protocol fixture in
`/tmp/hearthspace-daily-native-debug-3` received confirmation, rejected a competing
locker, verified blocked capture/input/accessibility and redacted live metadata,
then restored the previous focused window after owner unlock. Real `swaylock`
also reached confirmed lock. Its crash probe needs a rerun: process discovery
found both the main locker and its PAM helper and safely aborted before killing
either. The harness subsequently cleaned up and restored tty2. Two real headless
lock tests passed; the complete compositor unit suite now passes 164 tests.

Remaining work includes scoped Snap launch/recovery, real browser/editor/file
workflows, notification actions and authentication cancellation, repeated native
clipboard/daily acceptance, and moving PNG compression out of the display loop.
Physical external-monitor hotplug remains an untested hardware matrix case;
connector lifecycle fixes have fake-target regressions.

### Combined native application acceptance

`/tmp/hearthspace-daily-apps-native-4` ran on the same AMD display, tty7, with
2560×1600 physical pixels and scale 2. It verified:

- Catalog launch of both Snap Firefox and Mozilla's native Firefox. The Snap
  process tree belonged to the recorded scope and its session owner service.
- Browser Unicode paste and confirmation, exact browser-to-Foot clipboard bytes,
  local link navigation, and file selection with the expected file-content
  sentinel rendered by the browser.
- VS Code saving exact Unicode bytes, GTK Text Editor reopening and saving that
  file, and Nautilus creating a folder in the owned test workspace.
- A real MATE authentication prompt and cancellation without authorization;
  notification action selection returning exactly `open`; portal OpenURI returning
  success and opening the exact requested HTTPS address in the isolated browser.
- The complete Wayland/XWayland clipboard workflow, including both selection
  types, Unicode agent paste, and invalid-paste preservation. Fixture processes
  and windows disappeared afterward while the native session remained active.
- Forward/reverse keyboard window switching, maximize/fullscreen restoration,
  canvas pan/reset, and focused-close fallback with exact input received in the
  surviving terminal.
- Three screenshot rounds overlapping 274 control/input operations. Maximum
  observed status latency was 2.73 ms and input latency 1.83 ms; each capture had
  advancing page-flip metadata. Commit `82c870d` bounds PNG encoding work off the
  compositor thread; GPU readback remains synchronous.
- Confirmed real swaylock followed by killing only its identified main process.
  Thirty subsequent samples stayed locked, with capture, input, metadata, and
  accessibility protected. This tests fail-closed locker loss, not PAM password
  authentication.

Evidence is in `application-results.json`, `artifacts/clipboard-attached`,
`artifacts/capture-responsiveness`, `artifacts/keyboard-navigation.json`, and
`artifacts/locker-crash.json`, with screenshots and metadata retained. Early
OpenURI probes timed out because the temporary desktop entries had not been
registered in the portal's deliberately isolated data directory. The documented
probe registration now includes that directory; the successful third attempt and
original failures are retained. GTK's implicit trailing newline and Firefox's
Linux tab shortcut required correcting probe expectations, not application code.

This pass found a real accessibility serialization failure: Firefox supplied a
second `description` attribute, making unrelated XPath actions fail. Commit
`9f115a0` normalizes duplicate attributes while preserving canonical object
identity. Eleven library tests and live semantic actions across Firefox and VS
Code passed; the updated accessibility binary's hash is recorded separately in
this run.

Shutdown restored tty2 and preserved both T3 services, but initially could not
remove Nautilus's private GVfs FUSE mount. Commit `81bfcf0` extends exact-owned
mount cleanup and refuses unknown mounts before runtime deletion. The harness
retry completed. Independent checks found the Snap scope/owner and their cgroups
absent, no probe processes, no runtime, and the host's separate GVfs mount intact.
Both temporary browser profiles and desktop entries were removed; host browser
defaults stayed unchanged. A fresh normal shutdown with Nautilus remains required
to validate this fix without recovery retry.

Remaining acceptance work is native restart persistence, initial dialog bounds,
normal UI logout/GVfs cleanup, repeated daily smoke, and final complete checks.

### Native logout, restart, and final regression pass

`/tmp/hearthspace-daily-layout-native-5` verified that a GTK app requesting
1600×1200 and its oversized dialog both initially fit the 1280×752 usable logical
area below the bar. Commit `af6100f` supplies initial configure bounds and corrects
client-decorated dialog centering; real-client tests also confirm an intentional
later resize can exceed those initial bounds. A quiet owned WAV played through
the native session with `paplay` returning zero. Nautilus remained open during
shutdown, with its private GVfs mount present.

Keyboard navigation passed again. The session then recorded a terminal at canvas
position (7310, -4190), viewport (6858, -4452), and scale 0.8. Semantic UI logout,
including Cancel followed by confirmation, preserved that viewport and saved it
on normal compositor exit. Cleanup completed without retry, removed the private
portal and GVfs mounts, restored tty2, and preserved the host GVfs mount and both
T3 services. Evidence includes `bounds-results.json`, `audio-exit.txt`,
`artifacts/ui-logout.json`, `logout-pre-confirm.json`, and mount snapshots.

`/tmp/hearthspace-daily-layout-native-6` started with only the saved layout copied
into a fresh session directory. Before launching any apps, it had the exact saved
viewport and no automatically reopened clients. Newly launched Foot windows
reused their saved per-app slots, including the distant position, with the target
focused and visible. `artifacts/layout-restart.json` and its captures retain the
assertions. This establishes native restart persistence, not application-content
restoration.

The earlier harness stop had terminated its session bus before the compositor
could save layout. Commits `f675da4` and `30e312a` give the verified, pidfd-bound
supervisor a six-second graceful exit before existing forced recovery. Native6
explicit harness stop completed gracefully with Nautilus open, saved its final
viewport, removed owned runtime mounts, and restored tty2. Evidence is in
`graceful-stop-proof.json` and `session.log`.

The full Rust suite passed 377 tests with E2E and required AT-SPI enabled
(`/tmp/hearthspace-daily-phase5-full-tests.log`). All 122 Python tests then passed
with E2E enabled; nine additional graceful-stop tests and 17 launcher tests passed
after the Python-only shutdown fix. Full workspace Clippy with warnings denied,
formatting, Ruff, the native all-target build, and dependency checks passed.
Complete Python revalidation and repeated native/failure runs follow below.

### Repeated daily use and failure recovery

All three fresh runs, `/tmp/hearthspace-daily-repeat-1` through `-3`, passed every
daily-smoke step with notifications enabled: keyboard launch and focus, exact
Unicode paste, keyboard selection of a running window, off-canvas recovery,
maximize/fullscreen/restore, five advancing captures, responsiveness to an
unfinished settings client, semantic actions, and notification dismissal. Each
completed in about 33–34 seconds including startup and cleanup. The earlier
launcher-submit failure did not recur; its original artifacts remain available.

All five final harness cases passed under
`/tmp/hearthspace-daily-final-{success,watchdog,compositor-failure,agent-interruption,startup-failure}`.
The success case also checked visible move/resize and both software and hardware
cursor capture. The watchdog recovered a stopped compositor, the crash case
recovered a killed compositor, and agent interruption recovered independently of
the agent process. Every case restored tty2, stopped the owned scopes, removed
its runtime, and left both T3 user services active. The summary is
`/tmp/hearthspace-daily-final-recovery-proof.json`; each run retains full manifests,
logs, binary hashes, and assertions.

This milestone covers the tested AMD/eDP machine and the documented everyday app
paths. Physical external-monitor hotplug and other GPU families remain untested.
Real swaylock confirmation and fail-closed crash behavior passed; password/PAM
unlock was not exercised, while owner unlock has protocol-fixture coverage.
Snap Firefox launches and cleans up correctly through the catalog, but its
private document-portal restriction remains; Mozilla's native DEB supplies the
tested browser file/link workflow. Screenshot GPU readback remains synchronous,
although PNG compression is bounded off-thread and concurrent native control
passed. These limits do not expand this milestone into the entire roadmap.

Final validation passed all 131 Python tests with `HEARTHSPACE_E2E=1` and
`HEARTHSPACE_REQUIRE_ATSPI=1` (`/tmp/hearthspace-daily-final-python-e2e.log`), including
the shutdown changes. The latest Rust source passed all 377 tests, full workspace
Clippy, and the native all-target build; subsequent code changes were Python-only.
Final `cargo fmt --all --check`, Ruff check/format, and `git diff --check` passed.
An independent acceptance review found no remaining blocker for the scope above.
The daily-use milestone is complete; the original tty2 and both background T3
services are restored and no test session remains active.
