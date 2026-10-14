# Native Backend TODO

Forward-looking TODOs for Hearthspace's native `udev`/DRM/KMS backend.

Evergreen architecture lives in [../docs/DRM.md](../docs/DRM.md). Native VT test
commands and log interpretation live in
[../docs/NATIVE_TESTING.md](../docs/NATIVE_TESTING.md). The old step-by-step
bring-up plan has been removed because the first native backend milestone now
exists.

## Current Baseline

- [x] Nested `winit` remains the default development backend.
- [x] Native `udev` backend is feature-gated behind `--features udev`.
- [x] Native backend acquires a libseat session, opens the primary DRM device,
      wires libinput, renders through GBM/GLES, and presents with KMS.
- [x] Native rendering is vblank/page-flip paced.
- [x] Native clients can use renderer-backed dmabuf import.
- [x] Native shell startup is enabled.
- [x] Secondary connectors are advertised as Wayland outputs and rendered through
      independent KMS/GBM surfaces.
- [x] Native screenshots read the selected output's normal GBM/GLES render
      target and complete after its KMS page flip. The legacy PNG reply remains
      available alongside metadata-bearing `capture` replies.

## Validation To Run On Real Hardware

Checked hardware cases below retain their original evidence and hardware scope.
They are not blanket validation of later backend changes. Combined daily-use
acceptance and subsequent native runs are recorded in
[DAILY_USABILITY.md](./DAILY_USABILITY.md).

- [x] Run `cargo check`.
- [x] Run `cargo check --features udev`.
- [x] Run `cargo check --no-default-features --features udev`.
- [x] Run `cargo test`.
- [x] Run `cargo test --features e2e --test headless_control`.
- [x] Run native VT smoke without the shell:
      `target/debug/hearthspace --tty --no-shell --exit-after-ms 10000`.
- [x] Run native VT smoke with the shell:
      `target/debug/hearthspace --tty --exit-after-ms 15000`.
- [x] Run native VT smoke with Firefox, GNOME Calculator, or another heavier
      Wayland client and confirm no repeated KMS commit failures.
- [x] Switch away from the VT and back while Hearthspace is running.
- [x] Unplug/replug or add/remove a monitor and confirm Wayland output globals
      update without crashing.
      GPD Win Max 2 HDMI-A-1 hotplug test passed: Wayland output globals were
      advertised/disabled across plug/unplug cycles and the compositor exited via
      the emergency exit chord without panics or KMS commit failures.
- [x] Test on at least one non-VM real DRM stack and record GPU/driver/session
      details in the issue or PR that validates it.
      GPD Win Max 2 native test details: AMD Radeon 890M Graphics, `amdgpu`
      DRM 3.64 on Linux 7.0.0-27-generic, Mesa 26.0.3, libseat on `seat0`,
      eDP-1 at 2560x1600@60Hz.

## Output And Hotplug Work

- [x] Rebuild the primary KMS output if the selected connector/CRTC/mode changes
      at runtime.
- [x] Render to secondary outputs instead of only advertising their Wayland
      globals.
      GPD Win Max 2 HDMI-A-1 validation passed: secondary output rendered
      correctly through the native backend after `feat: render native outputs
      independently`.
- [x] Replace the temporary horizontal connector-order layout with an explicit
      layout policy.
- [x] Preserve or migrate pointer position/focus predictably when output geometry
      changes.
- [x] Keep Wayland output identity attached to its physical connector when the
      primary changes; promote an existing output instead of silently retargeting
      its global to another connector.
- [x] Expose no stale output after the last connector disappears, and allow
      active-seat rescans while scanout is paused for lack of outputs. A deferred
      VT-paused rescan is distinct from a real empty output set.
      These lifecycle changes are covered by fake-connector/global tests and the
      164-test compositor library run in
      `/tmp/hearthspace-daily-output-lock-unit-tests.log` (`2d177b5`).
- [ ] Repeat primary-output replacement and removal/reconnection of **all**
      outputs on hardware, including lock/capture requests during the transition.
      Earlier secondary HDMI hotplug evidence does not establish these cases.
- [ ] Add real output removal cleanup if Smithay exposes a better lifecycle than
      disabling globals.

## Native Screenshot And Readback

The agent workflow, preferred presentation-linked capture design, and acceptance
criteria are tracked in [AI_ENABLEMENT.md](./AI_ENABLEMENT.md).

- [x] Implement native readback with Smithay's renderer memory-export helpers.
- [x] Associate readback from normal output rendering with the matching output
      page flip; do not report an independent offscreen redraw as native
      presentation evidence.
- [x] Return output identity, frame ID, pixel dimensions, logical bounds, scale,
      transform, and cursor policy through `capture [output-name]`; preserve
      `screenshot` PNG-only compatibility.
- [x] Include the software/client cursor from the framebuffer or overlay the
      sampled hardware cursor once, clipped to its actual plane buffer.
- [x] Bound pending requests, readback dimensions, and presentation timeouts;
      reject inactive sessions and invalidate requests when outputs disappear or
      their configuration changes.
- [x] Buffer partial command replies through nonblocking socket backpressure,
      preserve deferred request order, and bound connection/input/output queues.
- [x] Add deterministic capture-state, cursor, metadata, and transport regression
      tests, plus an opt-in native smoke/recovery suite.
- [ ] Record a passing unattended native observe–act–verify run with retained
      PNGs, metadata, logs, and VT/process cleanup evidence on the VM DRM stack.
- [x] Validate native capture on real AMD hardware with a scaled output,
      hardware and software cursor paths, VT restoration, and all recovery
      cases. See the 2026-10-14 validation record in AI_ENABLEMENT.md.
- [ ] Extend capture validation to secondary outputs and live output removal
      during a pending capture on real hardware.

Track end-to-end validation and recovery evidence in
[AI_ENABLEMENT.md](./AI_ENABLEMENT.md); capture coverage on the available AMD
display does not establish coverage on other GPU or VM drivers.

## KMS Damage Clips

Goal: re-enable scanout damage hints only when the active DRM stack supports
them reliably.

- [x] Keep renderer-side damage tracking enabled; it is still useful for
      minimizing GLES redraw work into the GBM buffer.
- [x] Keep native KMS commits passing `None` for damage clips until a fallback
      path exists. The Parallels/virgl VM path produced repeated failures while
      running heavier clients such as Firefox/GNOME Calculator:
      `Page flip commit failed on device Some("/dev/dri/card1") (Invalid argument
      (os error 22))`.
- [ ] Debug whether the failing property is `FB_DAMAGE_CLIPS` support, clip
      rectangle shape/count, buffer age interaction, or virtual-driver behavior.
      The relevant Smithay path is `GbmBufferedSurface::queue_buffer`, which
      converts damage into `PlaneDamageClips` and attaches the resulting blob to
      the primary plane commit.
- [ ] Add per-output/per-device state such as `kms_damage_clips_supported`.
- [ ] When damage clips are attempted, retry the same queued frame once without
      clips after an `EINVAL` commit failure, then disable KMS damage clips for
      that output/device for the rest of the run.
- [ ] Re-enable clips only after VT smoke tests pass on real DRM hardware and on
      the Parallels/virgl VM, or after feature-detecting and disabling clips on
      drivers that reject them.
- [ ] Log renderer damage and KMS damage separately so future failures clearly
      show whether rendering succeeded and only the scanout hint was rejected.

## Session Protection

- [x] Implement `ext-session-lock-v1` with native acknowledgements gated by a
      protected frame's page flip on every connected output. Isolate input,
      captures, metadata, and semantic agent actions while locking or locked.
- [x] Cover competing lockers, owning-client unlock/focus restoration, role-only
      destruction, dead-client isolation, and stale capture invalidation with
      protocol and deterministic regressions.
- [x] Exercise an owning protocol fixture on native KMS: confirmation, competing
      rejection, blocked agent access, and focus restoration after unlock passed.
- [x] Launch the distribution swaylock client and observe confirmed native
      presentation. Authentication remains in its packaged PAM policy.
- [ ] Repeat real swaylock parent-process death on native hardware and verify
      the shield remains until the owned test session is stopped. Its PAM helper
      is a separate child process; a process-name count alone is not a kill target.
- [ ] Test lock/unlock across the broader output, VT, and GPU matrix.

See [session locking](../docs/SESSION_LOCK.md) for the protocol test client and
recovery boundary. Manual locking does not implement idle lock or suspend policy.

## Multi-GPU And Direct Scanout

- [x] Keep single-GPU rendering as the supported native mode. Additional GPU
      enablement requires separate ownership and recovery validation.
- [ ] Split render-node, scanout-node, and per-output allocator ownership further
      before enabling secondary GPUs.
- [ ] Add explicit GPU/device selection policy before opening non-primary GPUs.
- [ ] Evaluate Smithay's DRM compositor/direct-scanout helpers after normal
      composited rendering is stable.
- [ ] Add direct-scanout only after there is robust fallback to composited
      rendering.

## Input And Session Polish

- [ ] Test multiple real keyboard and pointer devices through libinput.
- [x] Test touch events on concrete hardware and add basic single-touch handling.
- [ ] Test tablet/switch events on concrete hardware and add support beyond not
      crashing.
- [x] Improve VT pause/activate behavior if real hardware exposes connector,
      libinput, or DRM-master edge cases.

## Non-Goals For The First Native Milestone

- [ ] X11/XWayland drag-and-drop and EWMH behavior beyond implemented window states.
- [ ] Direct scanout.
- [ ] VRR/HDR/color management.
- [ ] Complex multi-monitor layout policy.
- [ ] Runtime GPU selection UI.
- [ ] Tablet/switch integration and the wider touch-device hardware matrix.
