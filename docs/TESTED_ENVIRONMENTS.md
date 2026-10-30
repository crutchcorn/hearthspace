# Tested environments

Hearthspace has been tested in these environments. This records hands-on use,
not a certification of every device, application, or feature combination.

| System | Processor | RAM | Environment |
| --- | --- | --- | --- |
| MacBook Pro | Apple M4 Max | 64 GB | Ubuntu 26.04 in Parallels |
| ThinkPad T480s | Intel Core i5-8350U | 24 GB | Ubuntu 26.04 |
| GPD Win Max 2 (2025) | AMD Ryzen 370HX | 64 GB | Ubuntu 26.04 |

The MacBook's RAM is the host's capacity, not a stated guest allocation.
Virtualized display/input testing and native hardware testing cover different
paths. A successful run on one system doesn't establish another GPU's behavior,
a complete multi-monitor matrix, or long-term sleep reliability.

When reporting a new result, include the Hearthspace commit, kernel and graphics
driver, display arrangement and scale, input devices, and the workflow tested.
Keep detailed captures and logs with the issue or test artifact. Use this page
as the single inventory of named test systems; feature guides describe how to
repeat the checks.

## Hybrid GPU native acceptance, 2026-10-30

The GPD system above was tested with its AMD Radeon 890M and an NVIDIA RTX
3070 Ti connected through Oculink. The runtime corresponding to `87dc0817` used
kernel `7.0.0-42-generic`, NVIDIA `595.99.02`, and Mesa `26.0.8-1ubuntu0.3`.
All three outputs were active at scale 1: the internal 2560×1600 panel and
external 3840×2160 and 2560×3200 displays.

Isolated native sessions verified live connector coverage, real Foot content
on every output, movement, resize background exposure, pan/zoom marker geometry,
and selection of all three displays in Settings. Lock presentation was confirmed
with all three connected outputs still enabled at the observation boundaries.
Input came through the harness;
this was not a physical keyboard or touchpad test. Normal shutdown and a separate
injected startup failure restored the recorded original VT and removed owned
processes and runtime directories.

The user observed no flashing during an earlier fence-only motion run. Captures
from the three-output acceptance were visually checked, but readback cannot
establish the absence of physical scanout flashing. NVIDIA-to-AMD image transfer
fell back to CPU copying, which itself reads pixels even during intervals without
harness captures.

An initial shell surface-configuration failure was traced to `EMFILE` opening
`/dev/nvidiactl` immediately before Vulkan swapchain initialization failed. The
shell inherited a soft file-descriptor limit of 1,024; its live surfaces needed
more. The startup fix raises only the shell's soft limit to its inherited hard
limit. The original 44-item desktop catalog then passed with the production
shell, while the supervisor and compositor retained their original soft limits.
Short-run descriptor samples remained near 1,090; this is not a long-duration
leak test.

These checks do not establish suspend, physical unplug/replug, renderer failover,
or compatibility with another GPU/driver combination.

### Motion comparison

A CPU profile identified repeated shell-identity decoding and sorting-key
calculation. The `87dc0817` optimization rejects unrelated identity families
before decoding and computes each sorting key once per operation. It retains
the rendering effects and ordering rules.

Compared with `e282860f`, two optimized native runs used the same three outputs,
44-item application catalog, release profile, TRACE settings, and controlled Foot
window. Each run repeated five-second pan, zoom, and window-move phases three
times. Requests were synchronous, followed by a minimum 1/60-second delay, so
faster replies increased the number of delivered requests. These are control
round-trip times, not physical input-to-photon latency.

| Measurement | Before | After, across both runs |
| --- | --- | --- |
| Zoom request median, range across phases | 20.9–26.1 ms | 13.0–14.7 ms |
| Window-move request median, range across phases | 10.6–14.0 ms | 2.2–3.6 ms |
| Completed requests per approximately 45-second run | 1,440 | 1,783–1,789 |
| Compositor CPU seconds per run | 43.30 | 39.94–40.00 |
| External-output completion rates during window moves | 42–58 Hz | 58–60 Hz |
| Internal-output completion rates during window moves | 39–42 Hz | 55–57 Hz |

Completion rates count verified page-flip events handled by the compositor,
not distinct client frames or physical panel refresh. Pan varied between repeats;
zoom still saturated about one compositor CPU core and the internal display
handled roughly 35–43 completions per second after the change. CPU-copy transfer
remains a performance limitation. There were no harness captures during these
phases, but the transfer fallback performs internal readback. The measurements
support reduced motion overhead, not sustained 60-Hz motion on every display,
client callback throughput, or a controlled comparison with the ThinkPad.
