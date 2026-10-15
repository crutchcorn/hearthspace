---
name: hearthspace-native-control
description: Operate and validate Hearthspace on a real Linux VT with the native agent harness, including screenshots, input, app launch, accessibility, and recovery. Use for native desktop interaction or DRM/KMS presentation testing; ordinary headless regressions do not require this workflow.
---

# Hearthspace Native Control

Use the repository's `scripts/hearthspace-agent` CLI to control a dedicated
Hearthspace session while the agent stays in its background service or SSH
connection. Read [the native guide](../../../docs/NATIVE_TESTING.md) for command
details and [host setup](../../../CONTRIBUTING.md#native-agent-testing) when a
prerequisite is missing. Run commands from the repository root.

## Establish ownership before taking a VT

- Check `sudo -n true`, `cat /sys/class/tty/tty0/active`, and
  `loginctl list-sessions`. Record the original VT and ensure the agent and remote
  connection live outside the desktop session under test. Noninteractive sudo,
  systemd/logind with login PAM, a DRM device on `seat0`, and a spare VT are
  required. Report a failed prerequisite; never ask for a password in chat.
- Inspect the actual host; do not assume it is a VM, has a particular GPU, or
  should return to `tty2`. Record kernel, GPU/driver, virtualization status, and
  output scale when reporting hardware results.
- Coordinate the seat with other agents. Only one native harness owns it at a
  time. If the launcher reports an existing owner, inspect that session and its
  task before acting. Do not stop another agent's session or bypass the harness
  with manual `chvt`, broad process kills, or a second compositor.
- Run native validation separately from GPU-intensive headless suites. A capture
  timeout under concurrent load is a failed attempt; retain it and distinguish
  any later isolated success from a proven diagnosis.

## Start, observe, act, and stop

Choose a fresh artifact directory per run and use its absolute path for every
command. `--session PATH` identifies the launcher's session; `--socket PATH`
attaches control to an explicitly known compositor but cannot start or stop it.
Do not guess a host Wayland socket or reuse an earlier run's manifest.

Start builds the required binaries **before** switching VTs. Use `--no-build`
only after the relevant native build has passed; `binaries.json` records the
binary hashes. Startup waits for control/settings readiness, a mapped shell,
and a native capture. Select an absolute watchdog lifetime that covers the
planned interaction; it is not extended by activity.

A bounded example, including cleanup on startup or action failure:

```bash
(
  set -eu
  native_run=$(mktemp -d /tmp/hearthspace-native.XXXXXX)
  printf 'Native artifacts: %s\n' "$native_run"
  cleanup_native() {
    if test -f "$native_run/session.json"; then
      scripts/hearthspace-agent --session "$native_run" --timeout 30 stop > "$native_run/cleanup.json"
    fi
  }
  trap cleanup_native EXIT
  scripts/hearthspace-agent --session "$native_run" --timeout 45 start --watchdog 300 > "$native_run/start.json"
  scripts/hearthspace-agent --session "$native_run" status > "$native_run/status.json"
  scripts/hearthspace-agent --session "$native_run" launch foot
  scripts/hearthspace-agent --session "$native_run" screenshot "$native_run/desktop.png"
)
```

For interactive work across separate tool calls, retain the printed session path
and use a `finally`/cleanup path to issue `stop` when the work ends. The shell
example stops as it exits. The independent systemd watchdog covers agent death
or a frozen compositor; it does not replace normal cleanup.

Inspect a screenshot before choosing a target. `status` supplies output names
and window state; use global `--output NAME` when targeting a particular monitor.
Screenshots write both PNG and adjacent JSON metadata. Use commands such as
`launch foot` (or a desktop entry ID), `click X Y`, `drag X Y END_X END_Y`,
`scroll -15`, `key ctrl+a`, and `type 'text'` with the same explicit session.
Use each subcommand's `--help` for options. `type` supports US-layout printable
ASCII, newline, and tab; unsupported characters fail before any typing. Input
cleanup releases held keys/buttons on errors. Scrolling follows the compositor's
normal zoom rules, including Super and `--scroll-zooms`.

## Turn observations into assertions

- Coordinates default to global output-logical units, independent of canvas
  zoom. When selecting from a screenshot, pass
  `--units pixels --metadata /absolute/run/desktop.json` to `click` or `drag`.
  Use current metadata from the same session and output, especially after a
  display change. Images already have the display transform applied; conversion
  is `logical_origin + pixel / scale` on each axis.
- Actions wait for a fresh capture by default. `--save /absolute/run/after.png`
  preserves that frame. `--wait change` and `wait --change-from BEFORE.png`
  accept any pixel change, including cursor motion or animation; `--wait none`
  establishes command acceptance only. None proves the intended feature worked.
- Assert the specific result: exact text received by the destination app,
  changed window geometry plus matching visible content, a targeted image
  region, or an accessibility property. Pair semantic checks with screenshots
  when validating native presentation. Do not infer clipboard success merely
  from a shortcut returning successfully.
- Native captures complete after a page flip. Retain metadata with each PNG:
  dimensions, scale, output identity, frame ID, presentation type, and cursor
  source. Frame IDs are meaningful only within the same output and compositor
  process. A hardware cursor is sampled at render time; cursor inclusion alone
  does not prove application state changed.

For semantic control, read the native guide's **External Automation and
Accessibility** section and [E2E guide](../../../docs/E2E_TESTING.md).
`HearthspaceCompositor::attach` uses the recorded runtime, Wayland display, and
command socket; its `stop()` detaches without tearing down the native session.
Use `SessionAccessibility` with the manifest's private
`dbus_session_bus_address`, call `enable(timeout)` before discovery, then use
`snapshot`, `find`, or `activate`. Never fall back to the host desktop's AT-SPI
bus. The native launcher retains lifecycle ownership.

## Verify recovery and retain evidence

Issue `scripts/hearthspace-agent --session ABSOLUTE_PATH --timeout 30 stop`
even after an action fails. Stop is idempotent. Inspect its JSON and retained
`session.json`: `cleanup.complete` must be true, the recorded original VT must
be active, the owned session/service must have stopped, and its runtime directory
must be gone. Check the background agent/remote service still works. A failed
systemd unit can remain as a diagnostic record after its processes exit.

If cleanup is incomplete, inspect `session.log`, the manifest, and
`journalctl -u UNIT` for its recorded unit. Recover that owned session with the
harness before starting another run. Preserve evidence and report the unresolved
condition if recovery still fails; do not treat a stopped compositor as proof
that all session descendants or VT ownership were cleaned up.

For harness changes, the native guide documents `scripts/native-smoke` success
and startup/crash/watchdog/agent-interruption cases, their prerequisite build,
and full regression commands. Run the cases relevant to the change, serially.
Keep logs, command/results JSON, manifests, binary provenance, and PNG/metadata
pairs in the run directory. Report what was asserted, actual hardware details,
failures and cleanup outcome, and artifact paths. Record OS setup changes in
`CONTRIBUTING.md`; avoid duplicating that setup in this skill.
