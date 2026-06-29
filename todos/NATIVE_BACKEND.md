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
- [x] Secondary connectors are advertised as Wayland outputs, but only the
      primary KMS target is rendered today.
- [x] Native screenshots intentionally return a clear unsupported error until DRM
      readback exists.

## Validation To Run On Real Hardware

- [ ] Run `cargo check`.
- [ ] Run `cargo check --features udev`.
- [ ] Run `cargo check --no-default-features --features udev`.
- [ ] Run `cargo test`.
- [ ] Run `cargo test --features e2e --test headless_control`.
- [ ] Run native VT smoke without the shell:
      `target/debug/hearthspace --tty --no-shell --exit-after-ms 10000`.
- [ ] Run native VT smoke with the shell:
      `target/debug/hearthspace --tty --exit-after-ms 15000`.
- [ ] Run native VT smoke with Firefox, GNOME Calculator, or another heavier
      Wayland client and confirm no repeated KMS commit failures.
- [ ] Run `scripts/smoke-udev-gtk.sh` and confirm the GTK client renders upright.
- [ ] Run `scripts/smoke-udev-screenshot-command.sh` and confirm native
      screenshots return the expected unsupported-backend error.
- [ ] Switch away from the VT and back while Hearthspace is running.
- [ ] Unplug/replug or add/remove a monitor and confirm Wayland output globals
      update without crashing.
- [ ] Test on at least one non-VM real DRM stack and record GPU/driver/session
      details in the issue or PR that validates it.

## Output And Hotplug Work

- [ ] Rebuild the primary KMS output if the selected connector/CRTC/mode changes
      at runtime.
- [ ] Render to secondary outputs instead of only advertising their Wayland
      globals.
- [ ] Replace the temporary horizontal connector-order layout with an explicit
      layout policy.
- [ ] Preserve or migrate pointer position/focus predictably when output geometry
      changes.
- [ ] Add real output removal cleanup if Smithay exposes a better lifecycle than
      disabling globals.

## Native Screenshot And Readback

- [ ] Implement native readback for command-socket screenshots.
- [ ] Decide whether native screenshots should read back the current scanout
      buffer, render into a separate offscreen target, or use a backend-specific
      capture path.
- [ ] Add tests or smoke scripts for native screenshot success once readback
      exists.

## KMS Damage Clips

Goal: re-enable scanout damage hints only when the active DRM stack supports
them reliably.

- [ ] Keep renderer-side damage tracking enabled; it is still useful for
      minimizing GLES redraw work into the GBM buffer.
- [ ] Keep native KMS commits passing `None` for damage clips until a fallback
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

## Multi-GPU And Direct Scanout

- [ ] Keep single-GPU rendering as the only supported native mode until the
      single-output path is stable on real hardware.
- [ ] Split render-node, scanout-node, and per-output allocator ownership further
      before enabling secondary GPUs.
- [ ] Add explicit GPU/device selection policy before opening non-primary GPUs.
- [ ] Evaluate Smithay's DRM compositor/direct-scanout helpers after normal
      composited rendering is stable.
- [ ] Add direct-scanout only after there is robust fallback to composited
      rendering.

## Input And Session Polish

- [ ] Test multiple real keyboard and pointer devices through libinput.
- [ ] Test touch/tablet/switch events on concrete hardware and add support beyond
      not crashing.
- [ ] Improve VT pause/activate behavior if real hardware exposes connector,
      libinput, or DRM-master edge cases.

## Non-Goals For The First Native Milestone

- [ ] X11/Xwayland support.
- [ ] Direct scanout.
- [ ] VRR/HDR/color management.
- [ ] Complex multi-monitor layout policy.
- [ ] Runtime GPU selection UI.
- [ ] Tablet/touch hardware polish beyond not crashing.
