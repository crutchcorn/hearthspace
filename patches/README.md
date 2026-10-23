# Dependency patches

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
  [The shell evidence](../docs/SHELL.md#layout-editor-crash-2026-10-22) records
  hardware, artifacts, cleanup, and the broader suite's validation limits.
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
  a later popup configure shrinks the viewport. Repeated tray-menu opening on
  Intel UHD 620 produced GPU hangs; a CPU-preprocessing diagnostic captured NaN
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
  creation and the full ordinary tray workflow passed on Intel UHD 620 with
  normal GPU rendering; see the [native evidence](../docs/TRAY.md#repeated-popup-gpu-hang--2026-10-22).
- **Removal:** upgrade to a compatible unmodified upstream revision that passes
  the regression and repeated native tray-menu workflow. Remove the Masonry
  Git-source patch, vendor directory, and patch file, then refresh `Cargo.lock`.
  Preserve the independent Xilem and Masonry Winit patches until their own
  removal conditions are satisfied.
