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

- [ ] Keyboard launch, forward/reverse window switching, close, canvas navigation,
      and recovery of off-screen windows work with overlapping everyday apps.
- [ ] Newly mapped apps receive focus; activation matches actual focus; closing a
      focused app returns focus to a useful surviving window.
- [ ] Maximize/fullscreen and restore work on Wayland and XWayland, including a
      zoomed canvas; dialogs and menus remain reachable.
- [ ] Agents discover installed apps and live windows, target stable window IDs,
      enter Unicode text, and receive errors for failed actions. Screenshots and
      semantic state verify each action's intended result.
- [ ] Settings/control clients have bounded I/O. A stalled peer cannot indefinitely
      freeze input, display updates, or subsequent automation.
- [ ] A native session has working audio, ordinary application configuration,
      useful desktop service integration, and a clear way to exit safely.
- [ ] Frequently needed system interactions (notifications, files, links,
      permissions, and session protection) have working, documented paths.
- [ ] Useful spatial state survives a normal restart without automatically
      reopening or destroying application work.
- [ ] A repeatable native daily-work smoke test covers the supported workflows,
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
