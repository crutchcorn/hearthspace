# Session-owned xdg-desktop-portal-wlr patches

[Patch](xdg-desktop-portal-wlr-session-policy.patch) for upstream
[`xdg-desktop-portal-wlr` v0.8.4, commit `34153094662acd713241ca6cbbb20003ef67da5f`](https://github.com/emersion/xdg-desktop-portal-wlr/tree/34153094662acd713241ca6cbbb20003ef67da5f).
This is a local patch; no upstream issue or pull request has been submitted for it.

The shared user bus can activate the distribution's wlr backend after a
session-owned backend exits. That process lacks Hearthspace's explicit chooser
configuration. Separately, upstream attempts restoration before consulting
`XDPW_PERSIST_MODE`, so setting it to `none` does not prevent an existing valid
restore token from bypassing selection.

The patch adds two opt-in command-line controls, documented in `--help`:

- `--bus-name=NAME` selects the well-known name used for acquisition and the
  name-loss signal match. Its default remains
  `org.freedesktop.impl.portal.desktop.wlr`. Validation uses
  `sd_bus_service_name_is_valid` and rejects unique connection names (`:...`)
  before connecting to D-Bus or Wayland. The sd-bus provider must expose this
  function; libsystemd provides it starting with version 246.
- `--no-restore` carries a boolean in `xdpw_state`. After all selection options
  have been parsed, `setup_target` ignores restore data and sets persistence to
  `PERSIST_NONE` before choosing a target. The existing environment policy can
  only lower that value, so even `XDPW_PERSIST_MODE=permanent` cannot enable
  persistence. Selection uses the configured chooser. With the separate
  Start-session correction below, `Start` emits no restore data. With the flag
  absent, the upstream restoration path is unchanged.

The integration must supply a dedicated name in both the backend arguments and
the session's portal descriptor, and retain its explicit chooser configuration.
No D-Bus activation descriptor or systemd unit is added by this patch. A new bus
name alone does not isolate restore tokens: the frontend resolves them using
caller/application identity, and wlr recognizes the `wlroots` restore-data vendor.

The patch changes only `include/xdpw.h`, `src/core/main.c`, and
`src/screencast/screencast.c`. Refresh it against the pinned upstream commit using
a full-index Git diff of those paths, excluding the independent Start-session
correction below; preserve the upstream `LICENSE` and the notice below.
Installation provenance must record the resulting patch checksum
alongside the upstream revision and built binary checksum.

Acceptance requires the actual patched backend behind an isolated portal:
valid monitor restore data must invoke the chooser, cancellation must prevent
capture, and successful selection must produce no restore data even with a
persistent request and `XDPW_PERSIST_MODE=permanent`. Also cover frontend transient
and persistent tokens, unchanged flag-absent behavior, invalid bus names, and a
crash-gap request that cannot activate the distribution-name backend. Supervisor
mocks alone do not establish the restoration behavior.

Remove this patch when a compatible upstream release supplies both controls (or
equivalent policies) and passes those regressions without the patch. Update the
session invocation, dependency provenance, and installation receipt together.

## Preserve the matched session in Start

[Separate patch](xdg-desktop-portal-wlr-start-session.patch), based independently
on the same pinned v0.8.4 commit, adds `break` after `method_screencast_start`
finds the requested session and assigns its capture instance. It does not change
the policy patch or force a particular persistence response.

Without the break, `wl_list_for_each_reverse_safe` advances `sess` to the
container-of-list-head sentinel before `Start` reads
`sess->screencast_data.persist_mode`. Those reads do not refer to a real session.
The isolated backend probe observed `persist_mode=23277` and unexpected
`restore_data` despite `--no-restore`. Keeping `sess` at the match repairs both
response fields while preserving legitimate nonzero persistence modes. The
existing `!cast` guard still rejects a missing session or capture instance.

Upstream research on 2026-10-26 found the missing break still present in
[`master` at `c0255d7b047b7263629ab5a314661045ff7f65e3`](https://github.com/emersion/xdg-desktop-portal-wlr/blob/c0255d7b047b7263629ab5a314661045ff7f65e3/src/screencast/screencast.c#L580).
The later session reads were introduced by
[`8ec3184b`, "screencast: Implement restore token"](https://github.com/emersion/xdg-desktop-portal-wlr/commit/8ec3184b6b92d5215e151c36ebcfc103a9fe2c2a).
The inspected file history and relevant issue/PR search yielded no maintained
fix for this lookup defect. This correction is local, not an upstream backport.

Apply the policy and Start-session patches and record their checksums. Keep this correction
separate when refreshing either patch. Runtime acceptance must check an actual
`Start` response: no-restore returns mode zero without restore data, while the
flag-absent persistent control returns the selected session's nonzero mode and
restore data. Multiple sessions with different modes must not borrow each other's
policy. Removing only this patch provides the negative control; the erroneous
value is undefined and need not repeat `23277`.

Remove this patch independently once an upstream version preserves the matched
session and passes those response checks. The MIT notice below covers all three
patches, and the upstream `LICENSE` remains unchanged.

## Capture lifetime across resize, pause, and close

[Separate lifecycle patch](xdg-desktop-portal-wlr-capture-lifecycle.patch) applies
after the policy and Start-session patches on the pinned v0.8.4 source. Those
two patch files remain byte-identical. The native acceptance run captured a
window until resize, then stalled after a buffer-constraints failure. A later
monitor request exited wlr with `create_frame sent before the previous frame
was destroyed`. A fresh backend delivered 163 changing monitor frames over
23.64 seconds. These observations establish the failures, but do not prove that
an old session's state caused the later monitor failure.

Upstream review on 2026-10-26 found
[`master` at `c0255d7b`](https://github.com/emersion/xdg-desktop-portal-wlr/tree/c0255d7b047b7263629ab5a314661045ff7f65e3)
still affected. The lifecycle and Wayland-read changes adapt the open, unmerged
[PR #407](https://github.com/emersion/xdg-desktop-portal-wlr/pull/407), specifically
head [`521b8be9`](https://github.com/emersion/xdg-desktop-portal-wlr/commit/521b8be97dd93328d88d5ae9d094dc7f3297a5d3).
That proposal supplies a compatible repair and deterministic fixture sources;
its reported runtime results are upstream evidence, not Hearthspace validation.
[PR #405](https://github.com/emersion/xdg-desktop-portal-wlr/pull/405) destroys an
existing frame at the next creation, while
[PR #340](https://github.com/emersion/xdg-desktop-portal-wlr/pull/340) returns early.
Neither alone repairs timer ownership or buffer lifetime. No upstream mutation
was made for this integration.

The adaptation retains PR #407's cancellation-before-removal, held-buffer pause
behavior, immediate legacy-frame tracking, target lifetime through disconnect,
and explicit timer ownership. It keeps the small Wayland read helper in
`src/core/main.c`, avoiding a new build target: after poll reports readability,
read and dispatch queued events without waiting for a live callback. Late
events for destroyed frames must not block PipeWire renegotiation. A paused,
unfinished buffer stays owned until resume or removal; publishing its initial
zero timestamp can stall consumers when real timestamps follow.

Local additions enforce one scheduled or active capture at both scheduling and
timer dispatch, mark teardown before callbacks can rearm work, and restart
the driver graph after ext buffer-constraints failure. Removing its current
buffer while streaming arms the tracked retry timer instead of triggering
processing inline. Cast destruction clears both timer pointers through
their owning cancellation paths. Recoverable failure also clears per-frame
damage, while retaining each buffer's accumulated damage. These additions are
local changes, not claims about code merged upstream.

The deferred removal retry addresses an independent review finding. In
[PipeWire 1.6.2 `clear_buffers`](https://github.com/PipeWire/pipewire/blob/1.6.2/src/pipewire/stream.c),
removal callbacks run before the dequeue queue is cleared.
[Loop invocation](https://github.com/PipeWire/pipewire/blob/1.6.2/spa/plugins/support/loop.c)
can run synchronously on the loop thread. Triggering from the second removal
can therefore reenter processing and dequeue an earlier removed buffer whose
`user_data` is already null, leaving the cast stuck holding that invalid buffer.
The existing retry timer defers processing until the callback batch has returned
and retains starvation retry and pause/teardown cancellation. The active ext
constraints-failure restart remains unchanged.

Changed upstream paths are `include/screencast_common.h`,
`include/wlr_screencast.h`, `src/core/main.c`, and
`src/screencast/{ext_image_copy,pipewire_screencast,screencast,wlr_screencast,wlr_screencopy}.c`.
The internal API addition is `xdpw_wlr_frame_capture_cancel`; cast state gains
`capture_timer` and `destroying`. CLI, D-Bus, and restoration policy are unchanged.
Record this third patch checksum with the other two and the rebuilt binary.

Acceptance must cover changing window pixels through growth and shrink,
pause/resume before the first completed frame and during capture, removing the
current buffer with an armed timer or active frame, and closing each state
before opening a monitor stream in the same backend process. Require no
duplicate-frame error, no unfinished first frame on pause, no callback into a
freed cast, and sustained frames at the current dimensions. Also cover recovery
from buffer starvation and late Wayland events for destroyed objects. Cover
two-buffer removal with synchronous trigger-to-process reentry, an uncleared
queue containing the earlier removed buffer, and recovery after replacement
buffers arrive. The maintained [lifecycle check](../scripts/check-portal-lifecycle)
exercises real C callbacks with transport substitutes and real libwayland socket
handling; its [fixture sources](../scripts/portal_checklib/lifecycle/) retain the
upstream notice. It does not establish real PipeWire/portal, native pixel, or
resize acceptance. Run it against the patched source and repeat real stream
acceptance when refreshing this dependency.

Remove this patch independently when an adopted upstream revision supplies all
these lifetime and restart invariants and passes those regressions without the
local changes. Reassess the local additions if PR #407 lands: merely merging
that PR or either creation guard is not the complete removal condition. Preserve
the upstream MIT notice when refreshing the adapted code and fixture sources.

## Retained upstream notice

The upstream `LICENSE` is unchanged. Its notice is reproduced here with the patch:

```text
MIT License

Copyright (c) 2018 emersion

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
