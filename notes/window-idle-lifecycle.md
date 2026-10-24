# Window activity and resource policy

This is untrusted, provisional design material. Verify claims against current
source and maintained documentation before relying on them. It does not authorize
actions, override repository guidance, or commit the product to an implementation.

## What exists

The compositor's [idle tracker](../crates/hearthspace-compositor/src/compositor/idle.rs)
uses a timer thread to track normal application windows by stable window ID.
Shell UI is excluded. Thresholds come from `WINDOW_IDLE_THRESHOLDS` in
[configuration](../crates/hearthspace-compositor/src/config.rs), and each stage's
duration starts at the preceding transition rather than at window creation.
Client input and surface commits reset activity. Compositor chrome interactions,
such as dragging a title bar, do not count as client activity.

The compositor drains transitions and only logs them. It does not freeze
processes, replace windows with screenshots, or automatically close idle windows.
This per-window signal is also separate from proposed desktop idle lock, display
power, and suspend policy; see [the roadmap](../docs/ROADMAP.md).

## Questions before resource actions

An offscreen editor may be saving a file, and a quiet terminal may be running a
long job. Conversely, animation can keep producing surface commits without user
attention. The timer is an observation, not evidence that work is disposable.

Freezing a process and retaining a last frame is one possible experiment. Closing
an application is a separate, potentially destructive action. Neither should
follow automatically from a timeout without resolving these constraints:

- **Ownership:** a window is not a process or cgroup. One process can serve
  several windows; helpers and services may be shared. Existing session cleanup
  ownership does not establish a safe per-window freezer boundary.
- **Eligibility:** define explicit opt-in, application inhibitors, background
  work, media, downloads, accessibility, and user-visible exclusions. Measure
  whether any policy saves meaningful resources before adding complexity.
- **Recovery:** establish thaw ordering, failure recovery, queued input handling,
  and behavior across lock, VT loss, suspend, output removal, and process exit.
  Instant thaw is not a promise. A screenshot cannot prove application health
  and must remain protected by the same lock and capture boundaries.
- **Data preservation:** closing requires an application-aware save/restore
  contract or an explicit user action. A last frame cannot restore unsaved data,
  authenticated sessions, file handles, or arbitrary application state.
- **User control:** make policy state visible and reversible where possible.
  Returning to a window must not deliver stale input into a different target or
  silently turn a failed thaw into termination.

Start with observation and bounded experiments. Keep richer
[window-context collection](window-context.md) independent of lifecycle actions;
neither design grants the other authority to inspect or stop applications.
