# Xwayland surface input targets

The [source patch](xwayland-surface-input-targets.patch) adds the default-off,
rootless-only `-surface-input-targets` option to Xwayland 24.1.10. The exact
archive and patch checksums are in
[the dependency requirements](../data/xwayland-requirements.json). The upstream
[24.1.10 archive](https://xorg.freedesktop.org/archive/individual/xserver/xwayland-24.1.10.tar.xz)
is the source identity; no Git revision is inferred from a release tarball.
The upstream notices are retained in [Xwayland COPYING](xwayland-COPYING).

**Integration checkpoint:** the private candidate passes the focused regressions
below. Supported launch, packaging, full-suite validation and native acceptance
are still being integrated. This record does not establish an installed fix.

## Why this is needed

An infinite canvas can place an X11 window outside the finite X root. Passing
canvas positions directly to Xwayland makes visible titlebars stop receiving
input at negative or distant positions. Keeping managed backing origins at zero
avoids that clipping, but makes separate canvas windows overlap in the X root.
Stock Xwayland converts Wayland surface coordinates to root coordinates and
later uses global X hit testing; it can therefore deliver input to a different
backing window.

Raising the intended window on the XWM connection cannot order input sent on
the separate Wayland connection. A regression sending A's click and B's click in
one batch delivered both to B. Flushing or waiting for an unrelated X reply is
not acknowledgment that the earlier input was processed.

## Routing contract

The patch carries an incarnation-based surface identity with native pointer
events through MI queues, master-device copies, frozen events and saved replay.
It keeps producer position, logical pointer state and event delivery state
separate. Hit testing resolves the selected surface's current child tree while
retaining normal DIX grab arbitration. Retired identities cannot bind to a new
window that reuses the same XID.

Confinement still updates producer coordinates. A physical correction made
after live input omits a second ordinary motion notification only when the
pre-publication logical route is unchanged, remains valid after callbacks, and
has no intervening route, grab or freeze change. State updates and hit testing
still run. Native motions themselves are never deduplicated.

Pointer frame handling preserves scroll/button ordering and avoids counting
continuous and discrete/value120 representations twice. Gated repeated pending
deltas accumulate. If continuous scroll was already delivered before a button,
late value120 metadata cannot retroactively change that delivery; the frame
retains its continuous representation. Tests make this precision tradeoff
explicit. Default-off behavior is preserved.

Ordinary client warps and XTEST remain zero-context X input and use global X
semantics. A warp's source window is a condition, not a routing destination.
An active grab can determine the recipient without determining the pointer's
hit-test ancestry. This patch does not enlarge the X root: oversized backing
surfaces and override-redirect menus extending outside it remain limited.

## Verification and removal

The private candidate passed 34 `headless_control x11_interactive::` cases and
one `x11_popups::` case without compositor input-time raises. These include
negative and far-positive canvas positions, pointer/touch CSD move and close,
batched recipients, popup movement, grab replay, target lifetime, keyboard
replay and raw-motion boundaries.

A separate private Wayland server exercises genuine relative-pointer callbacks
and pointer locks with real X clients. It requires confinement positions
`149 → 148 → 147`, exactly one motion and raw event per input step, balanced
clicks, recipient isolation, cursor restoration, unlock and cleanup. Earlier
candidates fail its coordinate and duplicate-motion checks. The current
candidate passes, with both negative controls retaining their exact failures.

The 98 callback cases check ordered scroll/button records and compare 49
default-off traces with the pinned baseline. Their MI drain is substituted;
they establish callback-to-queue behavior, not DIX or hardware behavior.

No matching upstream fix or published report was identified during this
investigation. The inspected upstream 24.1.14 and main input paths still relied
on root hit testing; merely upgrading did not supply deferred target identity.
Remove this patch only after a compatible upstream implementation passes the
batched, frozen/replayed, lifetime, confinement and scroll contracts without
local changes. Update the source pin, receipts and launch gate together, then
repeat full and native validation. Do not remove the launch gate while keeping
overlapping backing origins with an unmodified server.
