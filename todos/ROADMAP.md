# Roadmap: Known Rough Edges & Deferred Scope

Feature-level TODOs moved out of the project `README.md`. These are
desktop-environment capabilities that are either partially implemented (rough
edges) or intentionally deferred. Native backend work lives in
[NATIVE_BACKEND.md](./NATIVE_BACKEND.md); accessibility/AT-SPI work lives in
[OTHERS.md](./OTHERS.md). Agent-driven native session testing is documented in
[Native Testing](../docs/NATIVE_TESTING.md), following [AI_ENABLEMENT.md](./AI_ENABLEMENT.md).

## Implemented Baseline

These entries describe implemented capabilities. The combined day-to-day
acceptance criteria and retained native evidence live in
[DAILY_USABILITY.md](./DAILY_USABILITY.md); a checked feature is not a claim that
every application, GPU, or output combination has been validated.

- [x] **AI enablement** — native screenshots, reliable control transport, a VT
      session launcher with recovery, and agent CLI/smoke tests are implemented.
      See the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md).
- [x] **Three-finger navigation** — touchpad and touchscreen gestures pan in any
      direction and zoom, including combined movement during a pinch.
      See [Pan and Zoom](../docs/PINCH_ZOOM.md) for behavior and native testing.
- [x] **Window navigation and recovery** — keyboard launch/cycling/close,
      focus activation, stable window actions, and off-canvas recovery.
- [x] **Standard window states** — Wayland and XWayland maximize, fullscreen,
      restore, and reachable transient dialogs at non-unit canvas zoom.
- [x] **Search and running-window selection** — the shell bar launches installed
      apps and searches live windows; agents can query both catalogs.
- [x] **Selection interoperability** — ordinary clipboard and primary selection
      bridge Wayland/XWayland; explicit agent clipboard/paste accepts Unicode.
      See [clipboard coverage](../docs/CLIPBOARD.md) for transfer limitations.
- [x] **Spatial persistence** — normal shutdown saves viewport and ordinary
      window positions, without reopening apps or saving their contents.
      See [window management](../docs/WINDOW_MANAGEMENT.md).
- [x] **Manual session locking** — Smithay session-lock support, swaylock/PAM,
      input and automation isolation, and native presentation acknowledgement.
      See [session locking](../docs/SESSION_LOCK.md).
- [x] **Native display/input backend** — DRM/KMS, libinput, explicit output
      layout, and independently rendered connectors. The remaining hardware
      matrix is tracked in [NATIVE_BACKEND.md](./NATIVE_BACKEND.md).

- [x] **Lid and suspend coordination** — lock-before-sleep, active-session lid
      policy, and bounded native wake recovery. See [power handling](../docs/POWER.md)
      and [hardware acceptance](../docs/POWER_TESTING.md).

## Known Rough Edges

- [ ] **Optional desktop protocols** — several optional protocols are not
      implemented yet, so clients may print warnings.
- [ ] **XWayland polish** — X11 apps can map and exchange clipboard data with
      Wayland apps, including primary selection. Cross-protocol drag-and-drop
      and EWMH states beyond implemented activation/maximize/fullscreen remain.
- [ ] **Full application/hardware compatibility** — ordinary profiles, host audio,
      private desktop services, notifications, and owned-session recovery have
      supported paths. Keep application-specific failures and untested hardware
      separate from those implementations; see [daily usability](./DAILY_USABILITY.md).

## Deferred Scope

- [ ] **Full login-session desktop environment integration.**
- [ ] **Minimization, workspaces, and richer task-management UI.** The existing
      shell bar and window selector cover basic launch and recovery.
- [ ] **Automatic idle lock and broader power management.** Remaining hardware
      acceptance and feature scope are tracked in [power management](POWER_MANAGEMENT.md).
- [ ] **Complex multi-monitor policy and multi-GPU rendering.** Existing output
      rendering/layout does not establish complete hotplug or driver coverage.
- [ ] **Theming** beyond the current proof-of-concept shell UI.
