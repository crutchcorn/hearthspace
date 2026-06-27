# Roadmap: Known Rough Edges & Deferred Scope

Feature-level TODOs moved out of the project `README.md`. These are
desktop-environment capabilities that are either partially implemented (rough
edges) or intentionally deferred. Backend work lives in
[BACKENDS.md](./BACKENDS.md); accessibility/AT-SPI work lives in
[OTHERS.md](./OTHERS.md).

## Known Rough Edges

- [ ] **Pinch-gesture zoom** — zoom currently supports shell buttons and
      `Super`-modified scroll, but there is no pinch gesture zoom yet.
- [ ] **Optional desktop protocols** — several optional protocols are not
      implemented yet, so clients may print warnings.

## Deferred Scope

- [ ] **Full login-session desktop environment integration.**
- [ ] **Minimization, task bars, workspaces, panels, or richer launchers.**
- [ ] **Persistence of window positions.**
- [ ] **Multi-monitor support.**
- [ ] **Theming** beyond the current proof-of-concept shell UI.
- [ ] **DRM/KMS backend and libinput device management** — tracked in
      [BACKENDS.md](./BACKENDS.md) (Step 3).

## Out of Scope

- **X11/Xwayland support** — intentionally out of scope unless a concrete need
  appears.
