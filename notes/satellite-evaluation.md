# Satellite evaluation

Status: experimental, 2026-11-05. This is provisional investigation material;
verify it against current source and [issue 68](https://github.com/crutchcorn/hearthspace/issues/68).

The normal X11 backend remains Smithay's direct Xwayland integration. The opt-in
`HEARTHSPACE_X11_BACKEND=satellite` starts the managed `hearthspace-satellite`
helper over a private Wayland socket. The helper must be built beside the
compositor, or selected with an absolute `HEARTHSPACE_SATELLITE_BINARY` path.
It is not yet included in the installed runtime inventory. Do not enable this
experimental backend in the regular login session.

## Recipient gate blocks adoption

The real headless `satellite::` tests use independent X11 connections, stock
Xwayland, Satellite, and Hearthspace. Negative `(89,-179)` and distant
`(50000,60000)` canvas positions receive balanced clicks at the requested local
coordinates. These tests do not yet establish titlebar move/close behavior.

The batched A/B test fails: both windows receive their separate control clicks,
but a single batch sends both complete clicks to B and none to A. Its desired
recipient assertions remain unchanged; the test is deliberately failing. A full
suite containing this test cannot currently pass. Do not skip or weaken it to
claim adoption readiness.

The [standalone reproduction](satellite-batched-input.patch) adds one integration
test to upstream revision `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`. It uses
Satellite's own synthetic Wayland host and real Xwayland, with no Hearthspace
code. Both independent controls pass; the single-flush batch reports:

```text
A=[]
B=[press(10,20), release(10,20), press(30,40), release(30,40)]
```

Apply the test-only patch to a separate checkout of that exact revision. With
a private mode-0700 `XDG_RUNTIME_DIR`, run its integration target filtered to
`downstream_batched_pointer_recipient_isolation`, serially with `--nocapture`.
Use the repository's guarded build and owned-test workflows. This patch is a
reproduction, not a dependency repair, and is not applied to vendored production
source.

The observed result is consistent with Satellite restacking on its XWM
connection before buffered input reaches Xwayland's later X hit testing.
Flushing or adding an unrelated X reply is not an input-consumption barrier.
[Upstream issue 146](https://github.com/Supreeeme/xwayland-satellite/issues/146)
describes an older stacking/focus defect that was fixed; it is related context,
not evidence that this batched case is already fixed. The sole upstream main
commit beyond the pin, `b424e5e`, changes Steam OSK role classification.

## Remaining work

Continue the optional cancellation bridge while keeping this backend
experimental. Cancellation cannot repair recipient routing. Before switching
defaults, resolve the failing batch and run popup, explicit/passive grab,
frozen/replayed input, destruction, scroll and lifetime regressions. Preserve
held-button ownership, delayed resize anchoring and per-interaction identity.

Clipboard, light/dark XSettings, cursor-size preferences, touch, zoom, multiple
outputs, X11 metadata, startup ordering, installation and native acceptance also
remain adoption gates. Do not retire the custom Xwayland checkpoint before the
replacement satisfies them.

Local retained evidence is under
`~/.local/state/hearthspace-investigations/20261105-satellite/`: `gates-2`
(two coordinate passes and the batch failure), `upstream-repro-3` (independent
batch failure), and `lifecycle-focused-1` (two helper teardown passes). Both
batch runs recorded empty, removed owned cgroups. Earlier failed build/setup
attempts remain retained. These are headless results, not native acceptance.
