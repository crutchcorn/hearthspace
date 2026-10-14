# AI Enablement Plan

Enable an agent running in T3 Code to start a full native Hearthspace session on
a spare Linux virtual terminal (VT), observe the presented desktop, operate real
applications, and restore the previous session without manual testing from the
user. The agent and its remote connection must remain available throughout.

This is planned work. The first milestone is a complete native
observe–act–verify loop, with screenshots and logs retained as evidence. Build
native capture and reliable transport first, then session orchestration and the
agent CLI, and finish with an automated native smoke test.

## Existing Foundation

- The native backend already uses libseat, DRM/KMS, and libinput, with rendering
  paced by page flips.
- The session supervisor starts the compositor, shell, and settings daemon with
  per-session runtime paths.
- The control socket already supports synthetic pointer motion, buttons,
  keyboard input, scrolling, app launch, screenshots, and shutdown.
- Nested and headless screenshots work; native screenshots explicitly return an
  unsupported error.
- The WayDriver adapter starts isolated headless sessions. Attaching it to an
  existing native session remains future work.
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

- [ ] Reuse Smithay renderer/readback facilities and inspect its existing native
      capture patterns before adding custom infrastructure.
- [ ] Capture a selected output and associate its pixels with the presented
      frame. Define behavior when there is no damage, presentation stalls, the
      output disappears, or the VT is inactive.
- [ ] Return output identity, pixel dimensions, scale, transform, logical output
      bounds, and a frame identifier with each capture. Define the conversion
      from screenshot pixels to input coordinates, including output offsets.
- [ ] Define cursor inclusion explicitly. Account for the separate hardware
      cursor plane and software cursor fallback without omitting or duplicating
      the cursor.
- [ ] Keep screenshot requests bounded, with clear errors for failed readback,
      lost outputs, and presentation timeouts.
- [ ] Fix command-socket reply buffering: retain unwritten header and PNG bytes
      across nonblocking partial writes and `WouldBlock`, then resume on socket
      writability. Handle client disconnects and bound queued memory.
- [ ] Preserve existing screenshot clients or provide an explicit protocol
      extension when introducing capture metadata.
- [ ] Add regression tests for partial replies, large images, disconnects,
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

- [ ] Build all required binaries before switching VTs, including the compositor
      with native backend support.
- [ ] Record the original active VT and allocate a spare VT without displacing
      an existing session. Prevent competing harness runs from claiming it.
- [ ] Start the complete Hearthspace session with isolated runtime paths and
      explicit native backend selection. Avoid inheriting display and session
      bus settings that direct clients to the host desktop.
- [ ] Define readiness as a responsive control socket, a presented native frame,
      and the expected shell/settings services being ready; a PID or socket file
      alone is insufficient.
- [ ] Improve supervisor signal handling, child cleanup, startup failure
      reporting, and readiness reporting where needed.
- [ ] Enforce an absolute startup deadline even while children continue logging.
- [ ] Record session identity, VT, socket paths, child processes, output metadata,
      and logs so the agent can inspect or stop the correct run.
- [ ] Provide idempotent teardown that stops owned children and their launched
      apps, releases native display resources, and restores the original VT on
      success or failure.
- [ ] Add an independent bounded watchdog for agent interruption or a hung
      compositor. The compositor's own exit timer is insufficient if its event
      loop has stopped progressing.
- [ ] Verify that T3 Code, remote connectivity, and the original desktop survive
      switching into and out of Hearthspace.

Acceptance: start and stop succeed from the background agent service. Startup
failure, compositor failure, timeout, and agent interruption all restore the
original VT and leave no harness-owned desktop processes running.

## Milestone 3 Agent Control CLI

Provide a small CLI over the existing control socket and launcher. The agent
can call it through terminal tools and inspect saved PNGs; an MCP wrapper is a
later integration. The command names below describe the intended interface and
are not implemented commands.

- [ ] Expose `start`, `status`, and `stop`, targeting an explicit session.
- [ ] Expose `screenshot`, saving an image and machine-readable metadata to an
      artifact directory accessible to the agent.
- [ ] Expose app launch, `click`, `drag`, `scroll`, `key`, and `type` operations.
      Define coordinate units and text/key encoding, and release held keys and
      buttons on failure.
- [ ] Return structured results and useful errors with bounded operation
      deadlines, so automation can distinguish command acceptance from success.
- [ ] Provide waits for presentation after an action and for observable UI
      changes. Do not treat an input acknowledgement or an arbitrary sleep as
      proof that the application repainted.
- [ ] Route synthetic scrolling through the same compositor zoom policy as
      physical scrolling, including modifier and `--scroll-zooms` behavior.
- [ ] Add regression coverage for input ordering, modifier cleanup, scrolling,
      screenshot-to-input mapping, and wait timeouts.

Acceptance: the agent can launch an app, inspect its screenshot, interact with
it, and verify the visible result using CLI output and saved artifacts alone.

## Milestone 4 Automated Native Smoke Test

Keep deterministic regression tests headless. Add a separate native suite for
seat ownership, KMS presentation, cursor capture, scaling, VT transitions, and
recovery. Document this distinction alongside the existing
[E2E testing policy](../docs/E2E_TESTING.md).

- [ ] Start the full session on a spare VT from the agent's background service.
- [ ] Launch a real client and type known visible text.
- [ ] Move and resize its window, then capture and verify the resulting content
      and geometry using the supported coordinate mapping.
- [ ] Verify cursor inclusion according to the capture policy, including
      hardware and software cursor paths where supported.
- [ ] Exercise output selection and scaling on the available display setup.
- [ ] Stop the session and assert restoration of the original VT and cleanup of
      all owned processes and runtime resources.
- [ ] Exercise startup failure, compositor failure, and watchdog recovery.
- [ ] Save screenshots, frame/output metadata, commands/results, and logs for
      failed runs as well as a successful reference run.
- [ ] Record GPU/driver/session details for native validation. Keep VM coverage
      and real-hardware coverage distinct.
- [ ] Run test, format, lint, and build checks before completing each
      implementation milestone, including native feature builds and relevant
      headless regression tests.

Acceptance: the entire workflow completes unattended, produces evidence of the
visible interactions, restores the original desktop, and keeps the remote agent
reachable. Recovery paths must pass before the harness is used routinely.

## Documentation and Host Setup

- [ ] Restore `docs/NATIVE_TESTING.md` with validated launcher, screenshot,
      control, teardown, artifact, and recovery workflows. Existing links already
      refer to this missing page.
- [ ] Document every required host change in
      [CONTRIBUTING.md](../CONTRIBUTING.md), including packages, installed service
      units, PAM/logind configuration, permissions, and lingering settings when
      applicable. Include how to undo harness-specific setup.
- [ ] Update [native backend TODOs](./NATIVE_BACKEND.md) as capture and session
      milestones land, keeping this plan focused on the complete agent workflow.

## Follow-up Integrations

- [ ] Before adding capture protocols, review the
      [Wayland protocol catalog](https://wayland.app/protocols/) and Smithay's
      helpers in the pinned dependency. Evaluate
      [image copy capture](https://wayland.app/protocols/ext-image-copy-capture-v1)
      for interoperability and reuse the native capture implementation.
- [ ] Add WayDriver attachment to an already-running native session, with
      explicit ownership so detaching does not accidentally terminate it.
- [ ] Extend existing AT-SPI support for semantic targeting such as finding a
      Save button. Keep accessibility discovery on the Hearthspace session's
      bus rather than the host desktop's bus.
- [ ] Add an MCP interface if it improves agent integration after the CLI
      workflow is proven.

Full remote desktop streaming, production login-session integration, and
unrelated XWayland polish are outside this first automation milestone.
