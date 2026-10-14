# Roadmap: Known Rough Edges & Deferred Scope

Feature-level TODOs moved out of the project `README.md`. These are
desktop-environment capabilities that are either partially implemented (rough
edges) or intentionally deferred. Native backend work lives in
[NATIVE_BACKEND.md](./NATIVE_BACKEND.md); accessibility/AT-SPI work lives in
[OTHERS.md](./OTHERS.md). Agent-driven native session testing is documented in
[Native Testing](../docs/NATIVE_TESTING.md), following [AI_ENABLEMENT.md](./AI_ENABLEMENT.md).

## Known Rough Edges

- [x] **AI enablement** — native screenshots, reliable control transport, a VT
      session launcher with recovery, and agent CLI/smoke tests are implemented.
      See the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md).
- [ ] **Pinch-gesture zoom** — zoom currently supports shell buttons and
      `Super`-modified scroll, but there is no pinch gesture zoom yet.
- [ ] **Optional desktop protocols** — several optional protocols are not
      implemented yet, so clients may print warnings.
- [ ] **XWayland polish** — X11 apps can map and exchange clipboard data with
      Wayland apps. Primary selection, drag-and-drop, and full EWMH state handling
      still need follow-up work. See [clipboard coverage](../docs/CLIPBOARD.md).

## Deferred Scope

- [ ] **Full login-session desktop environment integration.**
- [ ] **Minimization, task bars, workspaces, panels, or richer launchers.**
- [ ] **Persistence of window positions.**
- [ ] **Multi-monitor support.**
- [ ] **Theming** beyond the current proof-of-concept shell UI.
- [ ] **DRM/KMS backend and libinput device management** — tracked in
      [NATIVE_BACKEND.md](./NATIVE_BACKEND.md).
