# AI Enablement Plan

Enable an agent running in T3 Code to start a full native Hearthspace session on
a spare Linux virtual terminal (VT), observe the presented desktop, operate real
applications, and restore the previous session without manual testing from the
user. The agent and its remote connection must remain available throughout.

The native observe–act–verify loop is implemented, with screenshots and logs
retained as evidence. Native capture, bounded transport, session orchestration,
the agent CLI, WayDriver attachment, and private-bus accessibility are available.
The optional MCP wrapper remains deferred until an integration needs it.
The subsequent [daily usability milestone](DAILY_USABILITY.md) owns combined
application acceptance, desktop-service integration, and later regression runs.

## Implemented Foundation

- The native backend already uses libseat, DRM/KMS, and libinput, with rendering
  paced by page flips.
- The session supervisor starts the compositor, shell, and settings daemon with
  per-session runtime paths.
- The control socket already supports synthetic pointer motion, buttons,
  keyboard input, scrolling, app launch, screenshots, and shutdown.
- Nested and headless captures support deterministic regression tests. Native
  captures read the normal output framebuffer and complete after its page flip.
- The WayDriver adapter starts owned headless sessions or attaches to an external
  session without taking ownership of its lifetime.
- The preceding machine inspection found T3 Code running as a persistent user
  service with lingering enabled, a separate Tailscale service, and GNOME on
  `tty2`. Recheck these conditions at implementation time; discover the original
  VT dynamically instead of hardcoding `tty2`.

## Milestone 1 Native Capture and Reliable Transport

Capture should represent a frame submitted through the native display pipeline.
The preferred design is readback during normal output rendering, with the
request completed after the corresponding page flip. A separately rendered
offscreen snapshot cannot by itself prove that native presentation is working.
A page flip establishes display-pipeline progress, not that an application has
finished responding to input.

- [x] Reuse Smithay renderer/readback facilities and inspect its existing native
      capture patterns before adding custom infrastructure.
- [x] Capture a selected output and associate its pixels with the presented
      frame. Define behavior when there is no damage, presentation stalls, the
      output disappears, or the VT is inactive.
- [x] Return output identity, pixel dimensions, scale, transform, logical output
      bounds, and a frame identifier with each capture. Define the conversion
      from screenshot pixels to input coordinates, including output offsets.
- [x] Define cursor inclusion explicitly. Account for the separate hardware
      cursor plane and software cursor fallback without omitting or duplicating
      the cursor.
- [x] Keep screenshot requests bounded, with clear errors for failed readback,
      lost outputs, and presentation timeouts.
- [x] Fix command-socket reply buffering: retain unwritten header and PNG bytes
      across nonblocking partial writes and `WouldBlock`, then resume on socket
      writability. Handle client disconnects and bound queued memory.
- [x] Preserve existing screenshot clients or provide an explicit protocol
      extension when introducing capture metadata.
- [x] Add regression tests for partial replies, large images, disconnects,
      capture errors, and frame/coordinate metadata.

Acceptance: an agent can receive a complete PNG and its metadata from a selected
native output, and the reported frame corresponds to a successful page flip.
Cursor policy and coordinate mapping remain correct across supported scaling
and cursor presentation modes.

## Milestone 2 Native Session Launcher and Recovery

The launcher owns VT allocation, process lifetime, readiness, and recovery. It
must create a proper seat/session association for the compositor; switching VTs
with `chvt` alone does not establish that association. A systemd service with
PAM/logind integration and a controlling TTY is the proposed approach to
validate. Run the desktop as the user, with only the necessary host operations
privileged.

- [x] Build all required binaries before switching VTs, including the compositor
      with native backend support.
- [x] Record the original active VT and allocate a spare VT without displacing
      an existing session. Prevent competing harness runs from claiming it.
- [x] Start the complete Hearthspace session with isolated runtime paths and
      explicit native backend selection. Avoid inheriting display and session
      bus settings that direct clients to the host desktop.
- [x] Define readiness as a responsive control socket, a presented native frame,
      and the expected shell/settings services being ready; a PID or socket file
      alone is insufficient.
- [x] Improve supervisor signal handling, child cleanup, startup failure
      reporting, and readiness reporting where needed.
- [x] Enforce an absolute startup deadline even while children continue logging.
- [x] Record session identity, VT, socket paths, child processes, output metadata,
      and logs so the agent can inspect or stop the correct run.
- [x] Provide idempotent teardown that stops owned children and their launched
      apps, releases native display resources, and restores the original VT on
      success or failure.
- [x] Add an independent bounded watchdog for agent interruption or a hung
      compositor. The compositor's own exit timer is insufficient if its event
      loop has stopped progressing.
- [x] Verify that T3 Code, remote connectivity, and the original desktop survive
      switching into and out of Hearthspace.

Acceptance: start and stop succeed from the background agent service. Startup
failure, compositor failure, timeout, and agent interruption all restore the
original VT and leave no harness-owned desktop processes running.

## Milestone 3 Agent Control CLI

Provide a small CLI over the existing control socket and launcher. The agent
can call it through terminal tools and inspect saved PNGs. See
[Native Testing](../docs/NATIVE_TESTING.md) for the implemented command interface;
an MCP wrapper remains an optional later integration.

- [x] Expose `start`, `status`, and `stop`, targeting an explicit session.
- [x] Expose `screenshot`, saving an image and machine-readable metadata to an
      artifact directory accessible to the agent.
- [x] Expose app launch, `click`, `drag`, `scroll`, `key`, and `type` operations.
      Define coordinate units and text/key encoding, and release held keys and
      buttons on failure.
- [x] Add live app/window discovery, stable window actions, private accessibility
      snapshot/find/activate, and explicit UTF-8 clipboard/paste operations.
      Locking waits for protected presentation and blocks desktop automation.
- [x] Return structured results and useful errors with bounded operation
      deadlines, so automation can distinguish command acceptance from success.
- [x] Provide waits for presentation after an action and for observable UI
      changes. Do not treat an input acknowledgement or an arbitrary sleep as
      proof that the application repainted.
- [x] Route synthetic scrolling through the same compositor zoom policy as
      physical scrolling, including modifier and `--scroll-zooms` behavior.
- [x] Add regression coverage for input ordering, modifier cleanup, scrolling,
      screenshot-to-input mapping, and wait timeouts.

Acceptance: the agent can launch an app, inspect its screenshot, interact with
it, and verify the visible result using CLI output and saved artifacts alone.

## Milestone 4 Automated Native Smoke Test

Keep deterministic regression tests headless. Add a separate native suite for
seat ownership, KMS presentation, cursor capture, scaling, VT transitions, and
recovery. Document this distinction alongside the existing
[E2E testing policy](../docs/E2E_TESTING.md).

- [x] Start the full session on a spare VT from the agent's background service.
- [x] Launch a real client and type known visible text.
- [x] Move and resize its window, then capture and verify the resulting content
      and geometry using the supported coordinate mapping.
- [x] Verify cursor inclusion according to the capture policy, including
      hardware and software cursor paths where supported.
- [x] Exercise output selection and scaling on the available display setup.
- [x] Stop the session and assert restoration of the original VT and cleanup of
      all owned processes and runtime resources.
- [x] Exercise startup failure, compositor failure, and watchdog recovery.
- [x] Save screenshots, frame/output metadata, commands/results, and logs for
      failed runs as well as a successful reference run.
- [x] Record GPU/driver/session details for native validation. Keep VM coverage
      and real-hardware coverage distinct.
- [x] Run test, format, lint, and build checks before completing each
      implementation milestone, including native feature builds and relevant
      headless regression tests.

Acceptance: the entire workflow completes unattended, produces evidence of the
visible interactions, restores the original desktop, and keeps the remote agent
reachable. Recovery paths must pass before the harness is used routinely.

## Documentation and Host Setup

- [x] Restore `docs/NATIVE_TESTING.md` with validated launcher, screenshot,
      control, teardown, artifact, and recovery workflows. Existing links already
      refer to this missing page.
- [x] Document every required host change in
      [CONTRIBUTING.md](../CONTRIBUTING.md), including packages, installed service
      units, PAM/logind configuration, permissions, and lingering settings when
      applicable. Include how to undo harness-specific setup.
- [x] Update [native backend TODOs](./NATIVE_BACKEND.md) as capture and session
      milestones land, keeping this plan focused on the complete agent workflow.

## Follow-up Integrations

- [x] Before adding capture protocols, review the
      [Wayland protocol catalog](https://wayland.app/protocols/) and Smithay's
      helpers in the pinned dependency. Evaluate
      [image copy capture](https://wayland.app/protocols/ext-image-copy-capture-v1)
      for interoperability and reuse the native capture implementation.
- [x] Add WayDriver attachment to an already-running native session, with
      explicit ownership so detaching does not accidentally terminate it.
- [x] Extend existing AT-SPI support for semantic targeting such as finding a
      Save button. Keep accessibility discovery on the Hearthspace session's
      bus rather than the host desktop's bus.
- [ ] Optional: add an MCP interface when a concrete integration requires one.
      The proven terminal CLI provides the requested agent workflow already.

Full remote desktop streaming, production login-session integration, and
unrelated XWayland polish are outside this first automation milestone.

## Foundation Validation Record

This record describes the original AI-enablement milestone. Later capture-worker,
session-service, application, and shutdown changes have separate evidence in
[daily usability](DAILY_USABILITY.md); the historical counts below are not the
final daily-use test totals.

All required validation gates passed with the execution permissions needed for
Unix sockets and privileged VT operations. The WayDriver dependency uses the
same `async-io` executor as the compositor, and integration tests use a private
accessibility bus to avoid interference with the host desktop.

Native validation ran on 2026-10-14 on real GPD hardware, with AMD Radeon
880M/890M graphics using `amdgpu`, Linux `7.0.0-38-generic`, and `eDP-1` at
2560×1600 with scale 2. `systemd-detect-virt` reported no virtualization. The
harness allocated `tty7` and restored the discovered original `tty2`; both
`t3code.service` and `t3code-tailnet.service` remained active.

The unattended success case launched `foot`, typed the visible green
`HEARTHSPACE_NATIVE_MARKER`, verified its pixels and output file, moved the window
from (510, 220) to (462, 256), resized it from 696×524 to 611×472 logical units,
and retained screenshots after each operation. The terminal capture included
its software I-beam cursor (`cursor_source: framebuffer`); separate cursor
movement captures exercised `hardware-overlay`. Every native capture reported
`presentation: page-flip` and scale 2.

Startup failure, compositor `SIGKILL`, frozen-compositor watchdog recovery, and
owning-agent `SIGKILL` all passed separately. Each case confirmed original VT
restoration, stopped session scope, absent child PIDs, removed runtime directory,
and `cleanup.complete: true` without errors. Passwordless sudo was restored by
the machine administrator; the harness installed no persistent units or PAM
configuration. Host requirements and reversal are documented in CONTRIBUTING.

Evidence is retained locally under `/tmp/hearthspace-ai-evidence/`: the reference
run is `native-final`, invoked with the default smoke deadline after the final
DRM event fix. Earlier passing runs are retained as well. Recovery runs are
`final-startup-failure`,
`final-compositor-failure`, `final-watchdog`, and `final-agent-interruption`.
Each session retains its manifest and log, with commands/results and PNG/JSON
pairs under `artifacts/`. Failed earlier runs are retained for diagnosis too.
These temporary artifacts are not committed and can be removed after review.

The strengthened smoke suite also passed as `native-assertions-2`: pixel
landmarks followed the reported move, the resized window exposed the original
background, and both software-cursor regions changed on pointer movement.
`startup-assertions` verified the injected compositor failure inside its allocated
PAM/VT session, rejecting preflight failures as recovery evidence. Both runs
restored `tty2` with complete cleanup. The earlier `native-assertions` run hit
a transport capture timeout while a headless GPU suite was also running;
cleanup succeeded. The blocking operation was not identified, and concurrent
GPU workload robustness remains unverified.

This validates the available single AMD output. VM capture coverage, secondary
output capture, live output removal during capture, and other GPU drivers remain
separate hardware-matrix work in [NATIVE_BACKEND.md](./NATIVE_BACKEND.md). Output
selection, removal/invalidation, deadlines, cursor clipping, scaled and odd-sized
coordinate conversion, and backpressure also have deterministic regression
coverage. A timestamp cutoff rejects delayed DRM events from replaced output
surfaces before acknowledging a pending frame; regression tests cover both
Smithay clock domains and missing metadata. Live hot-plug remains part of the
separate hardware matrix.

The Wayland capture protocol catalog and pinned Smithay helpers were evaluated.
The control interface reuses Smithay framebuffer memory export and the normal
KMS presentation path. An `ext-image-copy-capture-v1` frontend can reuse this
capture boundary when interoperability is needed; adding a second frontend is
not required for this automation milestone.

Final checks passed: all 209 Rust tests with workspace `udev,e2e,test-apps`
features and real attached-session E2E enabled; Rust format, workspace Clippy
with warnings denied, full workspace build, and native-only feature check.
All 51 Python harness tests passed with real headless sessions enabled;
Ruff lint and formatting checks passed too. Recovery tests run separately from these
deterministic checks because they require privileged VT operations.
