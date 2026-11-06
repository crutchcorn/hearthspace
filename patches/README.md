# Dependency patches

## Portal-wlr session policy and capture lifecycle

The pinned external ScreenCast backend has an opt-in patch for a dedicated
D-Bus name and fresh source consent, a Start-response correction, and a capture
lifecycle repair for resize, pause, and teardown. See the
[policy patch record](xdg-desktop-portal-wlr-session-policy.md) for the upstream
basis, regression boundary, installation, and removal conditions.

## Winit Wayland destroyed-window updates

- **Dependency:** `winit` 0.30.13, published source preserved in
  [`vendor/winit`](../vendor/winit/). Its [provenance](../vendor/winit/PROVENANCE.md)
  records the archive checksum, exact upstream revision, and retained notices.
  Smithay's separate Winit 0.31 dependency is unchanged.
- **Patch:** [discard queued updates during window cleanup](winit-wayland-destroyed-updates.patch),
  backported from upstream commit
  [`250cd01ae8d83a8969265a14496fb6957b44a34e`](https://github.com/rust-windowing/winit/commit/250cd01ae8d83a8969265a14496fb6957b44a34e).
- **Reason:** creating and dropping a transient window in an application event
  callback can queue its initial Wayland configure after the current iteration
  has taken pending updates. Cleanup removes the window, but its queued update
  survives into the next iteration, where Winit unwraps a missing window entry.
  Hearthspace encountered this panic while dragging Canvas tools in the layout
  editor. Removing the destroyed window's pending updates preserves live-window
  events and avoids changing the shell's drag timing to hide the dependency bug.
- **Upstream:** [PR 4760](https://github.com/rust-windowing/winit/pull/4760) merged
  on 2026-10-15. Its final implementation removes updates during cleanup; the
  PR description also discusses an earlier guard-based proposal. As checked on
  2026-10-22, the latest stable release is
  [0.30.13](https://github.com/rust-windowing/winit/releases/tag/v0.30.13), and
  the fix has not shipped in a compatible release.
- **Verification:**
  `cargo test -p hearthspace-compositor --features e2e --test headless_control transient_shell_window_retired_in_callback_preserves_survivor_resize`
  starts an owned headless compositor and the shell's Winit dependency in a
  separate client process. An initial resize callback creates and drops a
  transient window. The test requires its destruction and a later maximized
  resize of the surviving window, beyond the stale-update failure point.
  On 2026-10-22 it reproduced the missing-window panic without the patch and
  passed with the backport, using llvmpipe and isolated process environments.
  Native acceptance passed 15 Canvas tools drags, frame grouping and removal,
  and subsequent keyboard search with an unchanged shell PID and no panic.
  The [shell lifecycle guidance](../docs/SHELL.md#layout-editor-crash-2026-10-22)
  describes the retained contract and how to repeat this acceptance workflow.
- **Removal:** upgrade the Winit version used by Xilem/Masonry to a compatible
  upstream release containing this fix. Remove the Cargo patch, vendored package,
  and patch file, refresh `Cargo.lock`, and rerun the regression and native
  layout editor workflow before removing this ledger entry.

## AccessKit AT-SPI adapter ordering

- **Dependency:** `accesskit_atspi_common` 0.18.1, vendored in
  [`vendor/accesskit_atspi_common`](../vendor/accesskit_atspi_common/).
- **Patch:** [keep adapters sorted by ID](accesskit-atspi-adapter-order.patch),
  backported from upstream commit
  [`fb7efa8cdbe6ebb48106f30c1c66163ebc781b49`](https://github.com/AccessKit/accesskit/commit/fb7efa8cdbe6ebb48106f30c1c66163ebc781b49).
- **Reason:** the four shell corners create separate Wayland windows. When AT-SPI
  activates, their first redraws can arrive out of creation order. AccessKit
  appended adapters to a vector but used binary search to find them, causing a
  panic followed by an abort during cleanup. Sorted insertion preserves that
  lookup invariant without changing shell window or accessibility behavior.
- **Upstream:** [issue 820](https://github.com/AccessKit/accesskit/issues/820) and
  [merged fix 821](https://github.com/AccessKit/accesskit/pull/821). As of
  2026-10-17, the latest compatible release is 0.18.1, and the fix has not shipped
  in a release. Only the `atspi-common` change is needed here: Winit creates
  adapters on the same event-loop thread, while their deferred activation order
  varies. The upstream Unix message-queue change addresses concurrent creation.
- **Verification:**
  `cargo test -p hearthspace-shell --test accesskit_adapter_order` registers
  windows in order 2, 0, 1 and checks accessibility enumeration, updates, removal,
  and reactivation. It fails against the original 0.18.1 implementation. Shell
  acceptance additionally exercises the real private AT-SPI bus.
- **Removal:** upgrade the AccessKit version used by Xilem/Masonry to a compatible
  upstream release containing this fix. Remove the Cargo patch, vendored package,
  and patch file, refresh `Cargo.lock`, and rerun the regression and shell
  acceptance with accessibility enabled before removing this ledger entry.

## AccessKit clipped-node registration

- **Dependency:** the same vendored `accesskit_atspi_common` 0.18.1.
- **Patch:** [refresh clipping-dependent registrations](accesskit-atspi-clipped-nodes.patch),
  a local fix in `src/adapter.rs` and `src/adapter/filter_updates.rs`.
- **Reason:** at 4× scale, Background settings initially places Photo fit and its
  explanatory text below the viewport. Scrolling changes a container transform
  while the descendants' node data stays unchanged. AccessKit's visibility filter
  then exposes those descendants through `GetChildren`, but their D-Bus objects
  remain unregistered because they received no node-update callback. Clients see
  `UnknownObject` for visible controls.
- **Behavior:** when geometry or filtering topology changes, revisit the old and
  new clipping scopes, including siblings that may gain the filter's offscreen
  neighbor exception. Process descendants before parent child-list events and
  deduplicate node updates. Match actual exposed ancestry through transparent
  containers. Ordinary value/text updates do not trigger a scope traversal; the
  extra work is limited to affected clipping scopes, with no persistent cache
  or public API change.
- **Upstream:** checked on 2026-10-20. The
  [package releases](https://crates.io/crates/accesskit_atspi_common/versions)
  still have 0.18.1 as the latest 0.18.x release. Current
  [adapter source at `a39329d`](https://github.com/AccessKit/accesskit/blob/a39329d281a9381510496c17e81e87763edbb8bb/adapters/atspi-common/src/adapter.rs)
  retains the same callback-only registration logic. The clipping filter was
  introduced in [PR 574](https://github.com/AccessKit/accesskit/pull/574).
  No existing issue or fix for this specific registration gap was found; no
  upstream report has been published.
- **Verification:**
  `cargo test -p hearthspace-shell --test accesskit_clipped_nodes --test accesskit_adapter_order`
  checks entering/leaving view after ancestor scrolling, viewport resize,
  sibling movement/reordering, nested clips, and simultaneous updates. It also
  checks balanced interfaces and registration before `ChildAdded` events.
  The initial three clipping regressions fail against unpatched 0.18.1.
  Validate the real Settings Photo fit control through the private AT-SPI bus
  after scrolling at 4× scale.
- **Removal:** remove this patch after a compatible upstream release passes
  these clipping regressions and the real 4× Settings workflow without it.
  Preserve the separate adapter-order patch until its own removal condition is
  met.

## AccessKit AT-SPI context-menu actions

- **Dependency:** the same vendored `accesskit_atspi_common` 0.18.1, published
  from upstream revision
  [`f40dfc01a0c0e76de535969f82fb35e19513737d`](https://github.com/AccessKit/accesskit/tree/f40dfc01a0c0e76de535969f82fb35e19513737d/platforms/atspi-common).
- **Patch:** [expose and dispatch context-menu actions](accesskit-atspi-context-menu.patch),
  a local fix in `src/node.rs` with inherited-action registration refresh in
  `src/adapter.rs` and `src/adapter/filter_updates.rs`. Apply it after the
  clipped-node patch. Paths are relative to the repository root; existing
  vendored formatting and repairs are preserved.
- **Reason:** shell app controls advertise and handle AccessKit's
  `ShowContextMenu`, but the AT-SPI adapter lists only `click`, exposes the Action
  interface only for clickable nodes, and always dispatches `Click` for index
  zero. Assistive technology therefore cannot request these context menus.
  Also, changing only a parent's advertised child actions does not update the
  unchanged children's registered interfaces, leaving the Action interface
  absent after addition or stale after the last inherited action is removed.
- **Behavior:** one supported-action mapping drives interface exposure, action
  count, names, and dispatch. `click` keeps index zero when present;
  `showContextMenu` follows it, or takes index zero on context-menu-only nodes.
  Preserve the consumer's parent-advertised child actions. Resolve the current
  action and original tree/node target under one tree read, releasing the read
  before calling the action handler. Unsupported, negative, and out-of-range
  indices return false without sending a request. Changes to parent-advertised
  Click or ShowContextMenu refresh direct children in the old and new filtered
  trees, including children below transparent containers. Reuse the existing
  update deduplication and clipping rules; unrelated parent changes do not
  trigger this refresh, and clipped subtrees remain excluded.
- **Upstream:** checked on 2026-10-30 against current main
  [`f3026222559c7f5f46d8f25cb7404de68eb149f0`](https://github.com/AccessKit/accesskit/blob/f3026222559c7f5f46d8f25cb7404de68eb149f0/adapters/atspi-common/src/node.rs).
  Its action enumeration, interface test, and dispatch retain the same
  click-only limitation. Searches of upstream issues and pull requests for
  `ShowContextMenu` and context menus found no matching AT-SPI fix; no upstream
  report has been published for this repair.
- **Verification:**
  `cargo test --locked -p hearthspace-shell --test accesskit_context_menu --test accesskit_adapter_order --test accesskit_clipped_nodes`
  exercises the real adapter action API and `ActionHandler`, including click
  plus context menu, either action alone, no mapped actions, invalid indices,
  removal/reindexing/re-registration, parent-only child-action changes,
  transparent containers, inherited actions entering a clip, and request
  targets in a grafted tree. The new regression crosses action
  enumeration and dispatch; it does not inject widget action requests or
  establish D-Bus/native acceptance. On the real private AT-SPI bus, enumerate
  the shell app control's actions, invoke `showContextMenu` by its enumerated
  index, and verify that its menu opens. Also verify default click behavior.
- **Removal:** remove this patch after a compatible upstream release passes
  the action regressions and real private-bus context-menu acceptance without
  it. Preserve the adapter-order and clipped-node repairs until their separate
  removal conditions are met.

See [vendored provenance and refresh steps](../vendor/accesskit_atspi_common/PROVENANCE.md).

## Xilem initial window properties

- **Dependencies:** `xilem` and `masonry_winit` 0.4.0, copied from the same pinned
  [`271a27a6`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88)
  Git revision into [`vendor/xilem`](../vendor/xilem/) and
  [`vendor/masonry_winit`](../vendor/masonry_winit/). Other Linebender packages
  remain upstream dependencies; their versions and features are unchanged.
- **Patch:** [initial window properties](xilem-initial-window-properties.patch).
  `WindowView::build` forwards its cached defaults to `NewWindow`, and Masonry
  uses those defaults when constructing the window's render root. Windows
  without an override still use the application's defaults. Runtime theme swaps
  retain the existing Arc-identity optimization.
- **Reason:** upstream applies per-window defaults only during rebuild when the
  Arc changes. A window created with the same cached Arc on every frame never
  receives it. Persisted dark Settings and shell popups created after a live
  theme change therefore inherited the old application-wide colors.
- **Upstream:** checked on 2026-10-20. The current
  [WindowView creation path](https://github.com/linebender/xilem/blob/main/xilem/src/window_view.rs)
  and [NewWindow](https://github.com/linebender/xilem/blob/main/masonry_winit/src/event_loop_runner.rs)
  still omit the initial override. Runtime swaps came from
  [issue 1819](https://github.com/linebender/xilem/issues/1819). No upstream issue
  or PR has been published for this local repair.
- **Verification:** `cargo test -p hearthspace-ui-components window_theme_tests`
  builds real `WindowView`s and paints their initial widget trees, checking a
  persisted dark window and a new popup after an existing window changes theme.
  Removing the `WindowView::build` forwarding makes these regressions fail.
  Native shell acceptance additionally checks real window creation and first
  presentation; the CPU test does not create Winit or GPU windows.
- **Removal:** upgrade to a compatible upstream revision carrying initial
  per-window defaults. Remove both Git-source Cargo patches, vendor directories,
  and the source patch; refresh `Cargo.lock` and rerun the CPU and real-window
  appearance checks. Keep the regression until upstream coverage replaces it.

## Masonry retired-window presentation

- **Dependency:** the same vendored `masonry_winit` 0.4.0 at Xilem revision
  [`271a27a6`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88).
- **Patch:** [cancel retired-window presentation](masonry-retired-window-presentation.patch),
  a local lifecycle repair in `src/event_loop_runner.rs`.
- **Reason:** a redraw or animation signal queues a native window handle. A
  subsequent action can reconcile the shell's windows and close that window
  before the batch drains. Masonry then unwraps the removed entry. On
  2026-10-23, moving layout items triggered this panic at the final redraw-batch
  lookup; the compositor survived and the session supervisor restarted the
  shell. Visible windows created and closed within one callback also leave
  stale entries in the first-frame queue.
- **Behavior:** closing a window removes its pending first frame. Deferred
  first-frame and redraw batches discard retired targets when consumed.
  Explicit invalid close calls and inconsistent live-window maps remain errors;
  surviving windows retain their redraws and input. This is independent of
  Winit's lower-level queued-configure cleanup patch.
- **Upstream:** checked on 2026-10-23. The current upstream revision is still
  the pinned revision, whose [runner](https://github.com/linebender/xilem/blob/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_winit/src/event_loop_runner.rs)
  contains both lifecycle gaps. No matching fix was found and no upstream
  report has been published. Callback-driven window removal is supported;
  [issue 1778](https://github.com/linebender/xilem/issues/1778) describes removing
  windows from the application's returned view list.
- **Verification:**
  `cargo test --locked -p hearthspace-compositor --features e2e,test-apps --test headless_control masonry_window_lifecycle`
  uses a real Masonry event loop and owned headless Wayland server. It queues
  redraw before self-close, animation before another window closes the target,
  and creation followed by retirement before first presentation. Each case
  requires a later resize, render and real pointer activation on the survivor.
  All three cases reproduced missing-window panics without the patch on
  2026-10-23 and passed with it. The resize comes from the compositor so the
  survivor must process a real configure after the retired batch has drained.
  Also repeat native mouse drags out of and into layout groups, verify the
  resulting placement, and require an unchanged shell PID.
- **Removal:** adopt a compatible upstream revision that passes these lifecycle
  regressions and native group-movement acceptance without this patch. Preserve
  vendoring and the initial-window-properties patch until that separate repair
  also meets its removal condition.

## Smithay XSettings publication

- **Dependency:** Smithay 0.7.0 at pinned revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f),
  copied into [`vendor/smithay`](../vendor/smithay/). Its
  [provenance](../vendor/smithay/PROVENANCE.md) records the imported files and
  standalone manifest normalization.
- **Patch:** [flush XSettings publication](smithay-xsettings-flush.patch), a local
  change to `XSettings::update`.
- **Reason:** `change_property8` queues the updated `_XSETTINGS_SETTINGS`
  property, but Smithay does not flush it. When appearance changes outside an
  X11 event callback, idle GTK3 clients keep their previous theme until unrelated
  X11 activity flushes the connection. Flushing at the shared publication point
  covers initialization, set, remove, and clear without a round trip.
- **Upstream:** checked on 2026-10-20. The latest
  [release is 0.7.0](https://github.com/Smithay/smithay/releases/tag/v0.7.0), and
  [current source at `24647fc8`](https://github.com/Smithay/smithay/blob/24647fc8d00131eb7f388efed26e35ea316b0018/src/xwayland/xwm/settings.rs#L192)
  still returns immediately after queuing the property. No matching upstream
  fix or issue was found; no report has been published.
- **Verification:** build the workspace binaries, then run
  `HEARTHSPACE_E2E=1 cargo test -p hearthspace-compositor --features e2e,test-apps --test headless_control xsettings_publishes_saved_startup_theme_and_idle_live_changes`.
  This starts a private settings daemon and XWayland, checks the saved startup
  theme, and waits for `PropertyNotify` after live Light/Dark changes without
  sending X11 requests that could mask the missing flush. Also verify an idle
  GTK3 client on the private XWayland display receives `gtk-theme-name` updates.
- **Removal:** upgrade to a compatible upstream revision that publishes
  XSettings changes promptly and passes these checks without the patch. Remove
  the Git-source Cargo patch, vendor directory, and source patch, then refresh
  `Cargo.lock`. Retain the idle-client regression.

## Smithay X11 interactive cancellation

- **Dependency:** the same vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
- **Patch:** [dispatch move/resize cancellation](smithay-xwm-moveresize-cancel.patch).
  Adds a default `XwmHandler::cancel_move_resize_request` callback and dispatches
  `_NET_WM_MOVERESIZE` direction 11 to it. Existing implementors need no changes;
  the compositor retains responsibility for ending and draining its operation.
- **Reason:** the [EWMH cancellation message](https://specifications.freedesktop.org/wm/latest/ar01s04.html)
  lets an application end an interactive operation, including when it sees the
  button release before the window manager establishes its grab. Smithay
  previously discarded this message, so Hearthspace could not honor it.
- **Upstream:** checked on 2026-10-22 against
  [master `24647fc8`](https://github.com/Smithay/smithay/blob/24647fc8d00131eb7f388efed26e35ea316b0018/src/xwayland/xwm/mod.rs).
  Its dispatch handles directions 0–8 and ignores cancellation. GitHub's upstream
  issue/PR search for `MOVERESIZE` returned only the unrelated active-window
  [PR 1863](https://github.com/Smithay/smithay/pull/1863); no cancellation fix was
  found and no report has been published.
- **Verification:** run
  `cargo test -p hearthspace-compositor --features e2e,test-apps --test headless_control x11_csd_`.
  Real x11rb clients advertise CSD through Motif hints, send move/resize requests
  after X button events, and check rendered bounds, `TranslateCoordinates`,
  synthetic `ConfigureNotify`, wrong-button rejection, unrelated popup teardown,
  and fresh clicks/drags after cancellation. The touch regression uses the same
  production touch tracker as physical input. The two `cancel_while_held` tests
  specifically exercise direction 11 for both move and resize.
- **Removal:** upgrade to a compatible upstream revision with cancellation
  dispatch, adapt Hearthspace's handler if its API differs, and rerun these
  regressions without this patch. Keep the vendored dependency and Cargo patch
  while the separate XSettings fix still requires them.

## Smithay touch cancellation after a frame

- **Dependency:** the same vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
- **Patch:** [cancel all touch targets](smithay-touch-cancel.patch), a local fix
  confined to `TouchInternal::cancel` in `src/input/touch/mod.rs`.
- **Reason:** `down → frame → cancel` leaves the client contact active because
  cancellation returns when no frame is pending. With a pending frame, it also
  skips unchanged contacts on other clients. A released contact awaiting its
  final frame can retain stale work after cancellation.
- **Behavior:** use the pending marker or allocate a new cancellation marker,
  visit all active and pending-up targets, and drain the cancelled slots.
  The existing `WlSurface` client marker deduplicates cancellation across
  surfaces and contacts. Completed up/frame sequences receive no cancellation;
  cancelled sequences receive no later frame, motion, up, or pointer event.
  `TouchDownGrab` continues to release its implicit grab through its existing
  cancellation path. Public APIs and explicit grab invariants are unchanged.
- **Upstream:** checked on 2026-10-23. GitHub's master revision is
  [`24647fc8d00131eb7f388efed26e35ea316b0018`](https://github.com/Smithay/smithay/commit/24647fc8d00131eb7f388efed26e35ea316b0018),
  whose [cancellation implementation](https://github.com/Smithay/smithay/blob/24647fc8d00131eb7f388efed26e35ea316b0018/src/input/touch/mod.rs#L668)
  retains both guards. The marker/deduplication rework in
  [PR 2067](https://github.com/Smithay/smithay/pull/2067) and pending-up framing in
  [PR 1425](https://github.com/Smithay/smithay/pull/1425) are already in this pin.
  Upstream issue/PR searches for `touch cancel` and `pending_frame` found no
  matching fix. No upstream report has been published for this local repair.
- **Verification:** run
  `cargo test --locked -p hearthspace-compositor --test smithay_touch_cancel`.
  The new in-process Smithay server uses real Wayland connections and
  `WlSurface` targets, including two clients and multiple surfaces. It checks
  framed cancellation, mixed framed/pending contacts, per-client deduplication,
  pending-up retirement, completed sequences, stale-input suppression, fresh
  slot reuse, implicit-grab release, and absence of pointer events. No renderer,
  compositor process, or existing Hearthspace test fixture is involved.
  The additional `headless_control touch_cancel::` and `power_confirmation::`
  regressions exercise cancellation through shell clients and modal input.
  All six passed with the patch. Restoring only the original cancellation code
  failed five framed, mixed-client and pending-up regressions; restoring the
  patch passed all six again. The three
  `interactive::client::touch::cancellation` headless regressions also passed
  through the production compositor. Native browser validation is separate
  from these deterministic protocol checks.
- **Removal:** adopt a compatible upstream revision that passes these protocol
  regressions and the compositor/native cancellation checks without this patch.
  Remove this patch file and ledger entry, preserving the vendored Smithay copy
  and its separate XSettings and X11 move/resize patches until their own removal
  conditions are met.

## Smithay dmabuf feedback format updates

- **Dependency:** the same vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
- **Patch:** [share updated dmabuf validation formats](smithay-dmabuf-feedback-formats.patch),
  confined to `src/wayland/dmabuf/{mod.rs,dispatch.rs}` with no public API change.
- **Reason:** `set_default_feedback` advertised new GPU formats while existing
  bound globals and buffer params retained an immutable creation whitelist.
  A client following the new feedback could receive fatal `InvalidFormat`
  before the compositor's import handler ran. Replacing the advertised global
  alone cannot repair existing resources.
- **Behavior:** share validation formats through a mutex and extend them before
  publishing new default feedback. Retain earlier advertised formats so stale
  allocations reach the import handler, which still decides compatibility.
  New legacy bindings receive only the current main-tranche formats. Never
  advertised Fourcc values remain protocol errors. `params.create` import
  failures remain nonfatal; `create_immed` retains its protocol-defined fatal
  failure behavior. Hearthspace separately updates all live surface-feedback
  states through Smithay's existing API, including roleless surfaces.
- **Upstream:** checked on 2026-10-30. Current upstream
  [feedback state](https://github.com/Smithay/smithay/blob/master/src/wayland/dmabuf/mod.rs)
  and [request dispatch](https://github.com/Smithay/smithay/blob/master/src/wayland/dmabuf/dispatch.rs)
  retain the immutable whitelist. No matching upstream fix was found and no
  report has been published.
- **Verification:** run
  `cargo test --locked -p hearthspace-compositor --lib --features udev compositor::udev::gpu::feedback::tests -- --test-threads=1`.
  These real Wayland regressions dispatch through Hearthspace's actual `App`,
  reusing its existing service-free test constructor. They cover disjoint GPU
  formats, new Fourcc values on already bound globals and preexisting params,
  nonfatal stale allocations,
  current-only legacy advertisement, version 4/6 surface feedback, destruction
  and client-disconnect cleanup without GPU changes, empty-format rejection,
  and the no-GPU startup global upgrade. Only GPU import is replaced at the
  pending-import drain with a recorded rejection; these checks do not establish
  native GPU import or presentation.
- **Removal:** adopt a compatible upstream revision that passes these protocol
  regressions without this patch, then remove its source patch and ledger entry.
  Keep the other Smithay repairs and vendoring until their separate removal
  conditions are met. Retain Hearthspace's surface-feedback refresh adapter.

## Smithay caller-owned multi-GPU intermediates

- **Dependency:** the same vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
- **Patch:** [caller-owned transfer cache](smithay-multigpu-transfer-cache.patch),
  an independent local repair confined to `src/backend/renderer/multigpu/`.
  Paths in the patch are relative to `vendor/smithay`; existing patches and
  notices remain intact.
- **Reason:** one manager cache slot per GPU pair alternates allocation sizes
  when two differently sized outputs share that pair. The measured baseline at
  Hearthspace `aba99007` recreated the intermediate on 1,598 output-switch
  transfers, while 111 consecutive transfers to one output reused it. The
  repair retains one intermediate per live output, without a retirement list.
  Those baseline observations are not a measurement of this repair.
- **Behavior:** opaque, non-Clone `TransferCache` owns the intermediate and its
  direct/CPU-copy choice, bound to both DRM nodes and renderer context IDs.
  `MultiRenderer::with_transfer_cache` selects that cache's borrowed slot.
  Existing callers keep the manager pair cache; both paths use the existing
  size/Fourcc invalidation and the same allocation, negotiation, drawing, and
  fallback implementation. Clearing the manager does not clear caller caches.
  Hearthspace's output owner retains target completion across draw/readback/KMS
  errors, waits before storage reuse, and resets storage and completion together
  on lifecycle changes or missing completion.
- **Upstream:** the latest inspected multigpu file commit
  [`347b2b30225b0425f0b44dc4c590e768f7dbe73e`](https://github.com/Smithay/smithay/blob/347b2b30225b0425f0b44dc4c590e768f7dbe73e/src/backend/renderer/multigpu/mod.rs)
  still has the pair-keyed `dmabuf_cache`. This is a local ownership extension,
  not an upstream backport; no upstream report is published by this repair.
- **Verification:** a separate run on a host meeting the fixture prerequisite
  below is required:
  `cargo test --locked -p hearthspace-compositor --features udev --test smithay_transfer_cache -- --ignored --test-threads=1`.
  These cases are explicitly ignored in ordinary workspace/CI runs to preserve
  software-only coverage. Report this invocation's executed/passed/failed counts
  separately from the default run's ignored count; a default pass does not
  establish transfer-cache regression coverage. See the
  [test guidance](../docs/E2E_TESTING.md#smithay-transfer-cache-regressions).
  The fixtures exercise real `GpuManager`, `MultiRenderer::render`, and
  `MultiFrame::finish`, with a counting allocator and distinct DMA-BUF handles
  at the device boundary. They cover alternating and same-size owners, the
  unchanged default pair behavior, size/Fourcc/node/context changes, drop and
  explicit invalidation, the real CPU fallback branch, failed waits, and
  completion retention after errors. The production output completion helper
  is included directly in this integration target. Smithay is excluded from
  the workspace, so tests in the vendored package would not be sufficient.
  Native mixed-size cross-GPU output acceptance and timing remain required; these
  boundary substitutes do not validate hardware behavior or performance.
  **Fixture prerequisite:** pinned drm-rs validates node construction through
  sysfs and has no synthetic-node constructor. The fixture reads two distinct
  validated `/sys/class/drm` identities (primary/render nodes of one GPU suffice),
  without opening devices or issuing GPU commands. An explicit `--ignored` run
  still fails with a prerequisite message if fewer than two exist; it does not
  silently skip or bypass validation.
- **Removal:** adopt a compatible upstream revision with caller-owned
  intermediates that passes these regressions and native acceptance without
  this patch. Adapt the per-output owner to that API and retain its completion
  dependency. Remove only this patch and ledger entry; preserve the other
  Smithay repairs and vendoring until their separate conditions are met.

## Masonry finite scrollbar geometry

- **Dependency:** Masonry 0.4.0 at Xilem revision
  [`271a27a6`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88),
  preserved in [`vendor/masonry`](../vendor/masonry/). Its
  [provenance](../vendor/masonry/PROVENANCE.md) records manifest normalization.
- **Patch:** [normalize and refresh Portal scrollbar progress](masonry-portal-finite-progress.patch),
  a local extension of the existing guarded scroll-range calculation. Use it
  for focus reveal and the public viewport setter, and recompute progress after
  layout clamps the viewport, including hidden scrollbars and unchanged offsets.
- **Reason:** focus reveal divides by zero when content fits the viewport exactly.
  The resulting NaN can remain in a hidden scrollbar, then become visible when
  a later popup configure shrinks the viewport. Repeated native tray-menu opening
  produced GPU hangs; a CPU-preprocessing diagnostic captured NaN
  coordinates already in the input path. The deterministic regression reproduces
  non-finite rounded-rectangle paint without creating a renderer or GPU device.
  Vello and its shader settings remain unchanged.
- **Upstream:** checked on 2026-10-22. Current
  [Portal source](https://github.com/linebender/xilem/blob/main/masonry/src/widgets/portal.rs)
  still has the unguarded reveal/setter divisions and layout progress TODO.
  Existing [PR 1596](https://github.com/linebender/xilem/pull/1596) supplies the
  guarded calculation and [PR 1587](https://github.com/linebender/xilem/pull/1587)
  invalidates resized scrollbar rendering; both are already in this pin.
  No matching compatible fix was found, and no upstream report was published.
- **Verification:** `cargo test -p hearthspace-ui-components touch_scroll`
  exercises real Portal focus reveal at an exact 298×153 fit, then resizes to
  298×18 and validates actual paint with `ValidatingSink`. The unmodified
  dependency fails with `NonFinite { what: "Geometry::RoundedRect" }`; the patch
  passes alongside the existing touch-scroll regressions. Native repeated menu
  creation and the full ordinary tray workflow passed with native GPU rendering.
  The [tray lifecycle guidance](../docs/TRAY.md#repeated-popup-gpu-hang--2026-10-22)
  describes the regression boundary and repeatable native checks.
- **Removal:** upgrade to a compatible unmodified upstream revision that passes
  the regression and repeated native tray-menu workflow, then remove this patch
  file. Retain Masonry vendoring and its Git-source patch while the Button
  keyboard repair below still requires them. Once both fixes are available
  upstream, remove the vendor directory and Cargo override and refresh
  `Cargo.lock`. Preserve the independent Xilem and Masonry Winit patches until
  their own removal conditions are satisfied.

## Masonry Button keyboard press ownership

- **Dependency:** the same vendored Masonry 0.4.0 at Xilem revision
  [`271a27a6`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88).
- **Patch:** [require a matching activation press](masonry-button-key-ownership.patch)
  in `src/widgets/button.rs`. A focused Button records a non-repeat Enter/Space
  press and activates once on its matching logical key and physical key-code
  release. Losing focus, becoming disabled or stashed, and window blur clear the
  press. Repeated downs cannot arm a newly focused button. Pointer and AccessKit
  activation retain their existing behavior. Hearthspace's `SemanticButton`
  forwards lifecycle updates to its embedded native Button so it shares these
  cancellation rules.
- **Reason:** Button previously activated on any Enter/Space release. If a power
  capability update disabled a focused Reboot row between down and up, focus
  recovered to Lock screen and the release sent `lock-session`. Button now owns
  its press across event phases; menu focus recovery and navigation remain active.
- **Upstream:** checked current source, file history, issues, PRs, and the latest
  release on 2026-10-23. The [latest Button source change, `ef3b6094`](https://github.com/linebender/xilem/blob/ef3b6094ede784d6df5dbdfa72d67e8280a9c619/masonry/src/widgets/button.rs)
  still activates on an unmatched release. [Release 0.4.0](https://github.com/linebender/xilem/releases/tag/v0.4.0)
  provides no newer fix. No compatible existing fix was found and no report was
  published.
- **Verification:** run
  `cargo test -p hearthspace-ui-components semantic_button` and
  `cargo test -p hearthspace-shell ui::session_controls::`.
  Shared-button regressions cover focus transfer, returning focus, repeat events,
  disable/re-enable, window blur, mismatched keys, and fresh activation afterward.
  Separate shell regressions hold Enter and Space on Reboot, revoke capability,
  dispatch the rebuild and focus recovery, then release. A fake command socket
  must receive no request and the fake power worker queue must remain empty;
  a fresh press still opens the expected confirmation.
- **Removal:** adopt a compatible unmodified upstream revision that passes both
  filters, then remove this patch file and ledger entry. Retain lifecycle update
  forwarding in `SemanticButton`. Keep Masonry vendoring and the Cargo override
  while the independent Portal progress repair still requires them.

## Smithay DRM compositor damage-clip policy

- **Dependency:** the vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
- **Patch:** [persistent KMS damage-clip opt-out](smithay-drm-damage-clips.patch),
  confined to `src/backend/drm/compositor/mod.rs` and the new `damage.rs` module.
  Patch paths are relative to `vendor/smithay`; other repairs remain independent.
- **Reason:** migrating native presentation to `DrmCompositor` must preserve
  Hearthspace's [no-KMS-damage policy](../docs/DRM.md#damage-tracking-and-kms-damage).
  The pin generates clips for both rendered primary frames and directly scanned
  out elements. Renderer damage and completion synchronization must remain active
  when optional KMS damage clips are disabled.
- **Behavior:** `set_damage_clips_enabled(bool)` defaults to enabled in both
  constructors. Call it with `false` before the first frame on each new native
  compositor. Disabling removes clips from current, pending, queued, and prepared
  frames, promotes queued/prepared frames to full plane updates, and retains the
  existing reset-pending flag until a successful full submission. An unchanged
  scene therefore still yields a frame that clears the property. The setter does
  not submit or replace buffers, alter damage history, or remove fences. It cannot
  change an already submitted atomic request. Buffer resets, state resets, and
  `clear()` preserve the setting; re-enabling permits future clip generation.
- **Upstream basis:** this is a local API extension of the pinned compositor,
  not an upstream backport. No upstream report is published by this patch.
  The existing atomic request builder explicitly writes a zero blob value when
  a configured plane has no clips, and when a plane is disabled; it is unchanged.
- **Regression boundary:** use the real `DrmCompositor` on a harness-owned DRM
  device or lease with atomic KMS and a plane exposing `FB_DAMAGE_CLIPS`. Resolve
  property IDs on that same device; record CRTC and plane handles, the submitted
  property/value pairs, commit flags, result, and matching completion. Inspect
  the built request passed to `atomic_commit` in
  `src/backend/drm/surface/atomic.rs` (`commit` and `page_flip`), distinguishing
  `TEST_ONLY` requests from actual submissions. For every configured or disabled
  plane exposing the property, the first successful submission after disabling
  must explicitly contain `FB_DAMAGE_CLIPS = 0`; omission is insufficient. Query
  that plane's properties after completion to confirm the resulting zero value.
  Later submissions while disabled must never restore a nonzero blob. Cover both
  rendered-primary and direct-scanout generation paths, disabling before the
  first frame, an unchanged scene after an enabled frame, and disabling while
  pending, queued, and prepared frames coexist. Drain pending/queued frames with
  real matching completions, then submit the prepared frame to detect stale
  reintroduction. Repeat after buffer/state resets and `clear()`, and after a
  failed submission followed by a successful retry. Confirm completion/user-data
  delivery and rendering changes still work; native presentation and existing
  transfer-completion regressions remain separate requirements. Default-enabled
  and re-enable cases require a suitable controlled device on which nonzero
  clip generation was actually observed. Do not change Hearthspace's native
  opt-out merely to obtain that baseline.
- **Coverage limits:** no new automated fixture accompanies this patch. The
  public compositor requires a concrete DRM surface; a device-free imitation
  or assertions about source spelling would not exercise KMS submission or the
  retained-frame lifecycle. Smithay tests are excluded from workspace runs.
  Workspace checks establish compilation and existing regression coverage, not
  execution of the property-inspection cases above. Record native evidence and
  any unexercised cases explicitly; an absent property is not a passing case.
- **Removal:** adopt a compatible upstream opt-out that passes these lifecycle
  and native property checks while retaining renderer damage and synchronization.
  Adapt the native caller and remove only this patch and ledger entry; preserve
  the no-KMS-damage policy and all other Smithay repairs until their independent
  removal conditions are met.
## Smithay applied surface state and subsurface transactions

- **Dependency:** the vendored Smithay 0.7.0 at revision
  [`5c2ba413`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f).
  The pin, manifests, and MIT notice remain unchanged.
- **Patch:** [apply committed surface and parent-owned state](smithay-applied-surface-state.patch).
  Paths are relative to `vendor/smithay`. The repair changes
  `src/backend/renderer/utils/wayland.rs` and
  `src/wayland/compositor/{cache.rs,handlers.rs,transaction.rs,tree.rs}`,
  adding the private `tree/parent_state.rs` helper. No public API is added.
- **Reason:** scale, transform, and damage commits without another attachment
  left renderer state stale; a parent commit could consume a synchronized
  child's uncommitted attachment; stacking requests changed the live tree before
  the parent committed; and an unrecognized non-null buffer was reset as though
  the client had deliberately detached it. These failures can display stale or
  prematurely committed cursor pixels, or conceal snapshot failure as absence.
- **Behavior:** apply buffer mapping and damage from committed surface state.
  Mapping/view changes damage the full element, and damage-only commits invalidate
  imported textures. Retain unrecognized non-null attachments with no surface view
  until replacement or explicit null. Collect only descendants' already committed
  transactions. Cache position and stacking on their parent, applying layout after
  all transaction surface caches and before any post-commit hook or compositor
  callback. Updating parent-owned positions also updates child cache snapshots so
  later child commits cannot restore stale coordinates. Stacking uses a resolved
  pending order copied into the parent transaction. Destruction removes only that
  member from pending and cached orders, preserving surviving moves even while
  blocked and preventing an old order from applying to a recreated role.
  Position updates retain weak role references for the same lifetime boundary.
- **Upstream basis:** this is a local adaptation of the pinned source, informed
  by [PR 2141](https://github.com/Smithay/smithay/pull/2141), which was open when
  inspected on 2026-11-01. Its parent-owned position handling, cache override,
  and transaction callback ordering informed those portions of this repair.
  This is not a merged upstream backport. The attachment-independent renderer
  updates, removal of implicit descendant commits, staged stacking with role
  lifetime checks, and preservation of unrecognized non-null buffers are
  independent local fixes. The private helper stages layout until transaction
  application rather than mutating the live tree while cached states merge.
- **Verification:** run
  `cargo test --locked -p hearthspace-compositor --features udev compositor::udev::cursor_snapshot::tests -- --test-threads=1`
  in the configured headless EGL environment. The real App/Wayland fixture and
  surfaceless GLES renderer exercise asymmetric scale and transform changes
  without attachment, pending requests, damage propagation and texture
  invalidation, pending child attachment versus child commit, parent-owned
  position and stacking, blocked/desynchronized transactions, descendant
  collection, callback visibility, role recreation, reference-role destruction
  before commit and behind a blocker (above/below with three overlapping children),
  and root/child failure
  recovery. The storage-failure cases substitute only the live wl_buffer's
  driver-recognition boundary; surface requests, commit application, renderer
  state, and recovery remain real. They do not emulate an actual driver fault.
  Tests never modify retained SHM pixels. These headless regressions do not
  establish native cursor-plane, KMS, or cross-GPU behavior. Record executed
  results separately; fixture source alone is not passing validation.
- **Removal:** adopt a compatible upstream revision that passes these applied
  state and protocol regressions without the patch, including the independent
  fixes not covered by PR 2141. Retain snapshot error/absence handling and
  regressions. Remove only this patch and its ledger entry; preserve all other
  Smithay repairs and vendoring until their own removal conditions are met.

## Satellite optional interaction cancellation

- **Dependency:** vendored xwayland-satellite 0.8.3 at
  [`7bf5e0dc`](https://github.com/Supreeeme/xwayland-satellite/tree/7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0).
  The version, upstream pin, MPL-2.0 notice and other repairs remain unchanged.
- **Patch:** [optional cancellation bridge](satellite-interaction-cancel.patch),
  with paths relative to `vendor/xwayland-satellite`. It adds a protocol XML and
  focused client helper, changes move/resize dispatch, and adds the existing
  workspace's wayland-scanner dependency. Hearthspace supplies the matching
  compositor implementation separately.
- **Reason:** upstream forwards XDG move/resize but explicitly does not implement
  EWMH Cancel. XDG shell has no cancellation request. This local optional
  extension carries the forwarded toplevel, seat and initiating serial; it is
  not a merged upstream repair and has not been published upstream.
- **Behavior:** consume the remembered operation on Cancel, accepting button-zero
  cancel messages before the left-button start filter. Remove stale records on
  unmap/destruction. Absence of the optional global preserves ordinary
  move/resize and reports unsupported cancellation. Hearthspace restricts the
  global to its private bridge and validates the accepted operation/grab before
  retaining final geometry and retiring only that interaction.
- **Verification:** the real App/Wayland filter
  `compositor::xwayland::satellite::interaction::tests` checks visibility,
  stale serials, wrong windows/seats, completed interactions and held-grab
  retention. Mapping/buffer geometry and physical input are fixture inputs;
  protocol dispatch and Smithay grabs are real. The real X11 filter
  `satellite::cancellation::cancel_top_left_resize` exercises delayed X resize
  and anchor retention through Satellite/Xwayland. The complete `satellite::`
  filter still fails batched recipient isolation and neighbor release ownership.
  Keep those assertions: this extension does not solve either defect. No native
  acceptance is implied. See [Satellite](../docs/SATELLITE.md) for remaining gates.
- **Removal:** replace this extension with a compatible upstream cancellation
  mechanism and rerun the same scope, lifetime, delayed-buffer and held-input
  regressions. Retain the adoption blockers until they are actually repaired;
  switching backend defaults is a separate decision.

## Satellite embedded child stdio

- **Dependency:** the same vendored xwayland-satellite 0.8.3 at `7bf5e0dc`.
- **Patch:** [embedded stdio hooks](satellite-embedded-stdio.patch), relative to
  `vendor/xwayland-satellite`. Adds defaulted RunData methods and uses them at
  Xwayland spawn. Default behavior remains inherited stdin/stdout.
- **Reason:** Hearthspace's embedding uses stdin for its private host connection
  and stdout for readiness. Allowing Xwayland to inherit either keeps an obsolete
  channel alive after helper death and can mix child output with readiness.
- **Upstream basis:** local optional API extension, not a merged backport.
  Hearthspace overrides only these two descriptors with null streams.
- **Verification:** the real helper subprocess test
  `nested_xwayland_cannot_keep_private_wayland_or_readiness_open` substitutes the
  Xwayland executable with a bounded recording process. It checks the actual
  inherited descriptors, kills only the helper, observes EOF on both private
  channels while the nested process survives, then verifies pidfd completion
  and reaps the owned leader. It does not exercise Xwayland protocol behavior.
- **Removal:** adopt an upstream embedding API with equivalent child-descriptor
  ownership, rerun this subprocess regression, and remove only this patch and
  its hooks. Preserve the independent cancellation and routing work.

## Satellite surface-target input and managed Xwayland

- **Dependencies:** Satellite 0.8.3 at `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`
  and Xwayland 24.1.14, with exact archive/patch hashes in
  [requirements](../data/xwayland-requirements.json).
- **Patches:** [surface-target input](xwayland-surface-input-targets.patch) and
  [managed executable](satellite-managed-xwayland.patch). The former reuses the
  input-routing checkpoint from `e6a31626` and additionally distinguishes
  internal button-state repair from application releases. The latter adds a
  defaulted Satellite executable-path hook after the embedded-stdio patch.
  Both are local repairs; no upstream backport is claimed.
- **Reason:** Satellite's overlapping X roots and input-time restacking still
  misroute a queued A/B batch. After compositor cancellation, Xwayland also
  synthesizes a button release on re-entry, which can reach a neighbor that
  never received the press. Carry surface incarnation through queued/replayed
  input; perform re-entry button repair through the same device-state queue
  without delivering invented core/XI button events or raw releases. Real
  releases retain ordinary grab arbitration and delivery. Default-off server
  operation remains unchanged.
- **Ownership:** the normal-user preparation script validates the archive and
  patch before compilation, records source/compiler inputs and stages a binary
  plus hash receipt beside the runtime. The helper verifies those exact bytes
  and requirements before launching; there is no stock-server fallback.
  Xwayland's notices are retained in [COPYING](xwayland-COPYING).
- **Regression:** real Satellite/Xwayland clients assert batched A/B press and
  release recipients, no neighbor release after cancellation, a subsequent
  working move, late resize anchoring, and negative/distant canvas input. The
  previous server checkpoint fixes batching but fails the neighbor assertion;
  the additional repair passes both. Helper tests cover receipt rejection and
  private descriptor lifetime; Python tests cover build provenance. These
  focused cases do not establish full compatibility or native acceptance.
- **Removal:** adopt upstream routing that passes recipient, frozen/replayed
  grabs, destruction, scroll, confinement and cancellation ownership gates;
  remove the server patch, pin/receipt gate and preparation machinery together.
  Remove the Satellite hook when upstream provides an equivalent explicit
  executable interface. Preserve its stdio and cancellation repairs until
  their independent removal conditions hold.

## Satellite host appearance and cursor preferences

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [host settings](satellite-host-settings.patch), relative to the
  vendor root and applied after the cancellation/stdio/managed-path repairs.
  The optional interface retains its MIT notice and upstream MPL notices.
- **Reason/behavior:** Satellite owns DPI XSettings, so replacing its manager
  to publish Hearthspace's light/dark theme would lose scale settings. An
  authenticated host global sends theme changes to Satellite's existing
  serializer. Initial host preferences are received before DISPLAY publication;
  updates flush without unrelated X11 traffic. Ordinary clients cannot bind.
  The helper supplies the shared 24-pixel absent-only cursor seed. Satellite
  does not subsequently mask legacy resource files with an Xft-only database.
  Existing resource properties and per-client XCURSOR_SIZE keep precedence.
- **Regression:** real settingsd/App/Satellite/Xwayland tests check saved dark
  startup and idle light/dark changes while retaining all three DPI integers.
  Actual libXcursor subprocesses check default, environment override, existing
  wildcard/empty properties, and legacy Xdefaults/Xresources behavior. Private
  global visibility is tested with a separate real Wayland client. Native
  appearance and toolkit artwork remain separate acceptance boundaries.
- **Removal:** use an upstream host-settings integration that passes those
  preference and startup-order cases, including legacy-resource preservation;
  then remove only this patch and matching host binding. Preserve the input,
  cancellation, executable and stdio repairs until their own conditions hold.

## Satellite selection control and ownership transitions

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [selection control](satellite-selection-control.patch), relative to
  the vendor root and applied after host-settings. This local repair retains
  the MPL notices and adds only Rustix's already-pinned `rand` feature.
- **Reason:** an X11 selection owner with no previous focused window was never
  exported through Satellite's serial-dependent ordinary data source. Use the
  standard ext-data-control protocol for X11 export. Smithay owns host protocol
  validation; only the authenticated bridge client sees its global. Host data
  is still received through ordinary data-device/primary offers; privileged
  control offers are discarded. A per-instance MIME marker prevents echo loops.
- **Ownership:** weak references identify each exported X selection; cancelled
  or destroyed generations cannot serve later owners. Ordinary offer adoption
  waits until the matching export retires. A queued X clear/destroy is checked
  against actual ownership before invalidating state. Fresh claims after a
  genuine unowned transition use current X time, rather than a former owner's
  obsolete timestamp. Clipboard and primary retirement remain independent.
- **Regression:** the pre-repair unfocused URI-list owner fails; it passes with
  the repair. The real GTK headless workflow exercises Unicode/cut/bulk transfer
  both ways, primary middle-click both ways, clipboard independence, old and
  active X-owner exit, and subsequent recovery. Its primary assertion failed
  before the timestamp/queued-clear repair and passes afterward. Separate real
  Wayland clients verify that cancellation, settings and data control are hidden
  from ordinary clients. These are headless protocol/application results, not
  native display or performance acceptance.
- **Removal:** adopt upstream equivalent source export and generation handling
  that passes those focus, loop, owner-exit and primary regressions without this
  patch; remove only this patch and the private manager registration. Keep the
  other Satellite/server repairs until their independent conditions are met.

## Satellite window identity

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [window identity](satellite-window-identity.patch), relative to the
  vendor root, after selection-control. Pin and MPL notices remain unchanged.
- **Reason/behavior:** preserve executable identity for bootstrap clients without
  WM_CLASS. Cache the process basename from the existing XRes credential PID;
  prefer a nonblank class, then instance, then that basename. Deleted/empty
  properties restore the fallback without panicking. Extra class fields are
  ignored. The host identifies X11 windows by its private bridge connection,
  rather than by spoofable class names; shell-like names retain application roles.
- **Regression:** real X11 clients spoof WM_PID, replace/delete class properties,
  and exercise cached frame syntax while preserving window identity and placement.
  Separate Satellite input tests assert X11 metadata. These headless checks do
  not establish native presentation or all toolkit metadata behavior.
- **Removal:** adopt equivalent upstream credential/class fallback passing these
  regressions; remove only this vendor repair. Keep host connection provenance
  and other patches until their independent removal conditions are satisfied.

## Satellite focus and maximization state

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [window state](satellite-window-state.patch), relative to the vendor
  root and applied after window-identity. Existing MPL notices remain unchanged.
- **Reason/behavior:** keyboard re-entry looked up focus hints on the seat, so
  globally-active clients received no WM_TAKE_FOCUS. Read the target window's
  hints and remember its toplevel for popup-focus restoration. Forward host
  maximization to both EWMH state atoms, preserving fullscreen and other states;
  decode the XDG state array as native-endian 32-bit values. A paired EWMH toggle
  makes one XDG request. The new XConnection maximization hook defaults to no-op
  for other library consumers. This is a local repair, not an upstream backport.
- **Host policy:** retain Hearthspace's existing X11 pointer fallback for touch,
  using private bridge provenance rather than its XDG wrapper's surface variant.
  Native touch support cannot be inferred for every client behind Xwayland.
- **Regression:** a real InputHint=false client reproduces missing TAKE_FOCUS
  before the repair and receives it on repeated host re-entry afterward. Real
  CSD/SSD tests check paired maximize toggles, restore geometry, and subsequent
  input. Actual touch taps and cancelled drags retain exact local coordinates
  and independent recipients. Late-resize fixtures hold every actual backing
  window and distinguish immediate viewport adoption from deferred buffer resize.
  Popup tests use backing-root coordinates with independently checked pixels.
  These headless tests do not establish native input or display behavior.
- **Removal:** use upstream focus/state handling passing those regressions;
  remove only this vendor repair. Keep the host's X11 identity and touch policy,
  and retain the other patches until their independent conditions are satisfied.

## Satellite reproducible build identity

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [build identity](satellite-build-identity.patch), relative to the
  vendor root, after window-state. The package pin and MPL notices are retained.
- **Reason/behavior:** Git discovery from vendored source reported the enclosing
  Hearthspace commit as Satellite's revision. Report a static package/local-base
  identity instead and remove the Git build script and its unused dependencies.
  The embedded helper's version output continues to give the complete base pin.
  This is a local packaging repair, not an upstream backport.
- **Distribution:** installed releases carry Satellite's MPL notice, provenance
  and complete modified vendor source archive, alongside the private server and
  its X.Org notices. The archive normalizes ownership/time metadata; build caches
  and Git internals are excluded. Installation/native preflight validates the
  private server's exact receipt and selected compositor companions before
  activation or VT acquisition. No host Xwayland replacement is installed.
- **Regression:** the workspace build and real-client Satellite gates use the
  rebuilt helper; descriptor lifetime and dependency admission tests remain
  active. Real staged bundles verify source contents, repeatable release identity,
  receipt modes, corruption rejection and custom compositor paths. These checks
  do not establish an installed login, a CI execution or native presentation.
- **Removal:** use an upstream vendoring-safe build identity with no enclosing
  repository discovery, then remove only this patch. Preserve required notices,
  modified-source distribution and each other repair until its own condition holds.

## Satellite explicit popup parents

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [popup parent](satellite-popup-parent.patch), relative to the vendor
  root, after build-identity. Pin, package and MPL notices remain unchanged.
- **Reason/behavior:** the upstream property reader discards WM_TRANSIENT_FOR for
  popup roles, and the hover/focus heuristic ignores their explicit parent. With independent canvas homes and overlapping
  X roots, a popup can appear on an unrelated window. Prefer its declared parent
  only while mapped with a live XDG role, excluding self-parenting. Otherwise
  retain the existing hover/focus fallback, also validated for live role state.
  Role classification is unchanged. This is a local repair, not a backport.
- **Upstream basis:** the pinned create_role_window implementation and upstream
  [positioning explanation](https://github.com/Supreeeme/xwayland-satellite/discussions/68)
  describe the hover/focus heuristic. This repair does not claim to resolve the
  separate dialog-role or orphaned-popup issues.
- **Regression:** real X clients map transient popups while another window has
  hover/focus, check visible parent-local pixels, then exercise frozen delivery,
  destroyed/reused XIDs, stationary scroll and foreign restacks. Existing popup
  movement tests retain their canvas-boundary and client-reposition assertions.
  Record executed results separately; source coverage is not native acceptance.
- **Removal:** use upstream parent selection passing those popup, lifetime and
  replay cases without the patch. Preserve the independent routing, interaction,
  selection and packaging repairs until their own removal conditions hold.

## Satellite desktop-file XDND bridge

- **Dependency:** Satellite 0.8.3, pin `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`.
- **Patch:** [desktop DND](satellite-desktop-dnd.patch), relative to the vendor
  root, after popup-parent. Pin, package and MPL notices remain unchanged.
- **Reason:** the pinned host data-device drag callbacks are empty. Retiring
  Hearthspace's direct XWM would otherwise lose desktop-file drops into X11.
  Upstream [issue 133](https://github.com/Supreeeme/xwayland-satellite/issues/133)
  remains open; this local URI-copy bridge is not a general DND backport.
- **Behavior:** standard Wayland data offers become XDND 2–5 URI-list Copy
  offers. Resolve the actual X backing origin and density, never canvas homes;
  round fractional positions to the nearest X pixel.
  Each drag owns a fresh source window; match source, target and offer identity
  through status, finish and conversion. Coalesce motion until status arrives,
  and wait for final status before XdndDrop. Revocation never finishes a drop or
  acknowledges a successor. Clipboard/primary ownership is untouched.
- **Transfer/lifecycle:** reuse Satellite's foreign selection and INCR paths,
  cap DND property chunks at 64 KiB, and match requestor plus property for INCR
  deletion events. Foreign reads now return errors instead of panicking.
  Undropped Wayland offers stay owned by SCTK through leave; retirement revokes
  acceptance. Dropped offers finish only on matching X success. One current drag
  is retained; replacing it rejects obsolete conversions rather than keeping an
  unbounded history. Other MIME formats/actions, XDND proxies and the reverse
  application-DND direction are not implemented by this repair.
- **Regression:** real App/Wayland/Satellite/Xwayland/XDND receiver tests assert
  exact encoded URI lists, actual INCR header/chunks/termination, unchanged icon
  positions, repeated successful drops, Escape, rejection, stale status/finish
  and conversion fields, lock/source removal, and no click/drop into a revealed
  replacement, plus local coordinates on a zoomed and panned canvas.
  Stale-message cases replay public X event fields through XSendEvent;
  they do not claim kernel queue scheduling. Ordinary clipboard workflows remain
  separate acceptance checks. Record execution results separately from fixture
  source; none of these headless checks establishes physical native behavior.
- **Removal:** adopt an upstream host-DND integration passing these copy, transfer,
  generation and cancellation cases, then remove this patch. Preserve the other
  compatibility repairs and source/notices until their independent conditions hold.
