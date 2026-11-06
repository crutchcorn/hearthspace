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

## Parallels graphics distortion and resizing, 2026-11-05

The MacBook Pro guest above reported aarch64 kernel `7.0.0-38-generic`,
Parallels virtualization, virtio-gpu PCI `1af4:1050`, Mesa `26.0.8-1ubuntu0.3`,
and GLES renderer `virgl (Apple M4 Max (Compat))`. One Virtual-1 output initially
ran at 1728×957, scale 1. The host OS, macOS 27, was user-reported.

The installed optimized release `77cd3564a0f06bad56c275229e605e5f8f9b67bb1b468517cbb797ac4c69b797`
and an original debug compositor produced whole-output corruption in captures.
The repeated original debug run failed a desktop-pixel assertion after its first
drag. An initial full-repaint workaround passed a bounded drag test but did not
survive subsequent host resizing; it is superseded by the ownership repair below.

Host resizing delivered preferred modes including 1072×768, 1976×768,
1728×783, 2232×768, 1328×768, and 1672×956. The initial backend exhausted
recovery after rejecting modeset timestamps older than the new output record.
A synchronous modeset followed by a verified ordinary flip restored completion.
Preserving the Wayland output during buffer replacement stopped the launcher
flicker observed by the local user. These changes alone still produced black
scene buffers after repeated mode changes.

Sharing the producer's GBM device with the presenter corrected the remaining
failure. With ordinary incremental scene repainting restored, `shared-incremental`
passed exact terminal input, 12 desktop-corner/content captures, 11 terminal
drags, four desktop selection gestures, and transitions through 1024×768,
1920×1080, 1280×800, and 1696×957 with the terminal retained. The mode exercise
used an explicit, temporary diagnostic selector for modes already advertised by
DRM, followed by udev change events; that selector and producer readback logging
were removed from the final implementation. These runs establish bounded native
pixels and client preservation, not physical smoothness or long-term reliability.

The source-level full checks before the resize changes passed formatting, Ruff,
Clippy, build, Python tests (746 run, 14 skipped), and dependency checks. The Rust
headless integration target passed 410 cases and failed 12 X11 pixel cases. One
failed X11 resize case also failed with the entire native backend excluded;
those failures are separate from the native resize changes. The earlier Rust
command stopped at the failing integration target, so it did not cover every
remaining workspace target. Full logs retain the exact commands and outcomes. The corrected ownership path
subsequently passed full formatting, Ruff, workspace Clippy and all-target build,
plus all 132 native-backend library tests. Those focused checks do not replace a
full workspace test result.

Evidence remains under
`~/.local/state/hearthspace-investigations/20261105-parallels-graphics/`, including
failed attempts, binary hashes, PNG/metadata pairs, diagnostic patches, exercise
scripts, and cleanup manifests. Early manifests record their original checkout
artifact location; those directories moved only after their sessions stopped.
Several first stop attempts reported `Device or resource busy`; idempotent
retries completed recovery and restored the original VT. The temporary debug
installation retained the previous optimized release.

The local user subsequently confirmed that repeated smaller/larger Parallels
window resizes, terminal dragging, and icon selection worked with the clean
installed debug release `f69e1b84a2ec` (ownership fix `2cc04bf`). This final
`shared-host-confirmation` run used real host preferred-mode updates, without
the diagnostic selector or forced full repaints. It preserved the desktop and
launcher, then restored tty2 and removed the owned session/runtime. This is
manual acceptance for the exercised workflow; suspend, locking and other GPU
configurations remain outside its coverage.

## Smithay cursor planes and pointer zoom, 2026-11-01

The GPD Win Max 2 (2025) listed above was tested with its AMD Radeon 890M
and an Oculink-connected NVIDIA RTX 3070 Ti, using the GLES/Smithay native
backend after removal of the compositor Vulkan bridge. Kernel
`7.0.0-42-generic` and NVIDIA `595.99.02` drove the internal 2560×1600 panel,
3840×2160 DP-8 and 2560×3200 DP-9, all at scale 1. The tested optimized
compositor SHA-256 begins `698fb72213c6`, built from `d691db41` plus the
recorded migration edits. Its build and source snapshot are retained under
`20261101-smithay-cursor-planes/presenter-release-3` and
`presenter-release-frozen-3`. Companion executables were separately hash-checked
historical debug binaries, not newly built release companions.

`native-cursor-2` passed five synthetic pointer phases on every output: named
canvas cursor, client artwork, content-boundary crossings, window dragging and
desktop marquee dragging. Exact publication, plane-selection, accepted-queue,
worker-completion and main-presentation records associate named and client
artwork with hardware cursor planes on all three outputs. Across the six
named/client phases, 206 completed submissions advanced the cursor while
retaining the preceding scene and primary buffer/damage tuple. This establishes
cursor progress without another scene composition for those submissions; it
does not establish input processing during a blocked main loop.

The release atomic requests explicitly set AMD primary plane 202's
`FB_DAMAGE_CLIPS` property 22 to zero, followed by matching accepted queues and
verified completions; live property observation agrees. NVIDIA's primary and
cursor planes did not expose that property. Renderer damage remained active.
`native-cursor-nvidia-2` also passed all fifteen phases with NVIDIA confirmed as
the scene renderer, exercising Smithay's CPU-copy fallback from NVIDIA to
the AMD panel with real Foot content and window movement. No Vulkan compositor
transfer was selected.

`native-zoom-1` passed six Super+wheel phases and three Ctrl controls across all
outputs. Across 91 Super observations the maximum per-axis mouse-anchor drift
was below 0.497 canvas units; every zoom-in/out pair returned to its original view.
Captured green markers grew and returned with the projected window geometry.
All six phases sampled animation, but status samples do not establish continuous
physical presentation. Ctrl+wheel preserved the canvas view while Foot could
resize its own content. Coordinates were harness requests, not observed physical
mouse coordinates.

All three runs restored the original VT, stopped their owned services/processes
and removed private runtimes. Temperature enforcement was explicitly disabled
at the user's request while telemetry and the 120-second total diagnostic
budget remained active. The retained failed attempts are not passing evidence:
`native-cursor-1` saw a late battery widget; `native-cursor-nvidia-1` saw the Apps
widget finish expanding after Foot mapped. Subsequent diagnostics waited for
sampled layout stability before measuring and retained strict phase checks.
In the zoom run, the Apps widget widened from 54 to 102 logical units between
setup and the first-wheel preflight; all 14 layout queries from that preflight
onward were stable.

The tests do not establish physical mouse smoothness, FPS, input-to-photon
latency, GNOME/COSMIC parity, or a measured before/after performance improvement.
Client cursor source tags identify client-provided artwork, not an exact Foot
Wayland surface. Captures occur outside motion and do not show the held marquee.
Lock, hotplug, sleep and long-duration behavior were not exercised by these runs.
The installed session was not changed.

The source files subsequently committed through `18d9b66a` passed full Rust
formatting, Ruff lint/formatting, workspace/all-targets Clippy with warnings
denied, and the all-targets debug build. The matching `presenter-full-2` run
passed 2,539 Rust tests, with 26 ignored, and 745 Python tests, with one skipped.
The skipped guard fixture requires two permitted CPUs; the runner used CPU 4
alone. All 22 transfer-cache cases normally ignored by the workspace run passed
separately in `presenter-transfer-cache-build6`; those use substituted device
operations and do not establish physical GPU import. Dependency checks passed
with 74 categorized warnings: seven unmaintained, one yanked, seven Git-source
allowance and 59 duplicate-crate warnings, plus a missing-config/default-fallback
warning. These runs requested software rendering but did not retain an actual
renderer identity, so they are not evidence of software-only execution.

Full-run source endpoints matched across 2,340 paths, and each phase verified
empty, removed cgroups. The Rust attachment and Python shell fixtures now
wait for inherited process-group cleanup; their assertions and cleanup both
passed. The transient OSD workflow also passed after isolating its application
catalog and checking input/capture before accessibility traversal. Earlier
`presenter-full-1` and `presenter-python-1` failures remain retained failures.
Four fresh libXcursor regressions verify the new X11 24-pixel default and
environment/resource preference preservation. They measure requested size,
not VLC's rendered artwork in an installed session. This later X11 default was
not present in the frozen compositor used for the native runs above.

## Hybrid GPU responsiveness investigation, 2026-10-31

The Vulkan-transfer observations below describe historical code. Current native
transfers use [Smithay's DMA-sharing and CPU-copy routes](DRM.md#outputs-and-scaling);
the historical runs do not validate that change.

After reboot, the same three-output GPD setup selected AMD as its scene GPU.
Debug runs used Smithay's direct AMD-to-NVIDIA DMA sharing; they did not exercise
the new NVIDIA-to-AMD Vulkan route. Harness-injected typing was visibly slow,
and some runs repeated characters until their exact terminal receipts failed.

The decoder change at `c5b6f791` approximately halved the first two matched
command intervals in two diagnostic repeats. Those intervals include receipt
checks, not individual key latency. The per-output transfer cache at `7f20b524`
then reduced intermediate creation-path entries from 3,540 to two startup
entries. In the common first-three-command window, median external-display
transfer times fell from about 6.9 ms to 0.45–0.50 ms. The separate 22 cache
fixtures and 70 native-backend unit tests passed. Typing stalls and receipt
failures persisted, so these results do not establish an overall responsiveness
or native acceptance pass.

One later stall coincided with Tctl reaching 99.875°C and all CPU policies
reporting approximately 600 MHz for about ten seconds. The compositor remained
near one CPU's worth of execution, with little runqueue waiting; no quota
throttling or new memory-pressure events were recorded. This supports thermal
or power limiting as a contributor without identifying the limiter. An earlier
stall preceded telemetry. Sampled CPU-frequency values are estimates, and this
observation does not explain the historical reboot.

Both cache runs restored the original VT and removed their owned processes and
runtime directories after failure. Their logs, captures and timing records are
retained in the persistent `20261031-vulkan-resume` investigation directory.
Both runs stopped through normal failure cleanup; neither exercised a
temperature-triggered stop. Physical input and full Vulkan presentation remain
unvalidated by these runs.

The input-redraw repair at `c75bd217` and startup-renderer preference at
`c4d20c0a` passed 118 focused library cases and 18 headless interaction cases.
A subsequent debug diagnostic used a reviewed 85°C operational cutoff and
stopped during startup at 85.625°C, 8.5 seconds after monitoring began. The
typing workload did not start, so this attempt establishes no responsiveness
improvement. Both recovery layers completed, restored tty2, and removed the
owned runtime and processes. The retained run is `native-input-8` under the
same persistent investigation directory.

A second bounded startup, `native-nvidia-9`, selected NVIDIA explicitly and
logged Vulkan transfers to AMD's internal-display output. There were 88 transfer
passes and 88 verified eDP presentations before the 85°C cutoff, reached nine
seconds after monitoring began; nine more of each occurred during shutdown.
No Smithay cross-GPU completion or CPU-fallback message appeared. The retained
page-flip capture shows only background and cursor, without application content.
Transfer traces, presentation counts, and that capture are separate evidence;
they do not establish correct changing pixels or input responsiveness.

The retained wrapper and declared executable match the build records, but startup
was interrupted before live executable path/hash verification completed. Both
recovery layers again restored tty2 and removed their owned processes and runtime.
Two DRM previous-state restoration errors remain in the teardown log despite
completed recovery. The workload never began, so neither startup attempt is full
native acceptance of the new transfer or application-preference UI.

The per-window identity syntax cache at `928e919b` passed 25 focused regressions
covering identity changes, XDG unmap/remap, X11 fallbacks, live output resolution,
and authorization. The matching `full-6` source snapshot passed formatting,
linting, the all-targets debug build, 2,465 Rust tests and 721 Python tests.
Rust reported 26 ignored cases, including the 22 DRM-identity transfer-cache
fixtures whose separate passing run is recorded above. Dependency checks passed
with 74 categorized warnings, unchanged from `full-5`. The headless renderer
reported NVIDIA OpenGL ES despite requested software-rendering environment
variables; these results are not software-only coverage. No native latency
improvement has been measured for the identity cache.

The bounded application-preference runner passed 64 isolated fixtures covering
its input cleanup, accessibility errors, receipt ordering, selector provenance,
and thermal monitor. Those fixtures do not establish native UI acceptance.
Two subsequent guarded optimized-build attempts stopped at the unchanged 85°C
operational cutoff, with sampled peaks of 86.375°C and 85.25°C. The second began
at 57.25°C. Both stopped their owned units and verified removed cgroups; neither
produced a completed compositor executable or started a native session. Evidence
is retained as `optimized-compositor-1` and `optimized-compositor-2` in the same
persistent investigation directory. Optimized native transfer and app-preference
acceptance remain pending; these stops do not identify the reboot's cause.

## Release-build interruption, 2026-10-31

The GPD system rebooted automatically during a release build of the compositor
and shell at `aba99007`, on AC power with default BIOS/power settings according
to the user. New native acceptance had not started. The retained
journal ends without a shutdown sequence or a recorded OOM kill, GPU fault,
thermal trip, or panic near the reset. The next boot reports a software-issued
CF9 reset and unclean filesystem/journal recovery. This identifies the reset
mechanism, not its initiator or cause; missing final logs do not exclude a fault.
Crash capture was configured, but no crash dump was retained.

The compositor release artifact was unusable after reboot. The last resource
sample preceded the build, so it cannot establish peak memory or temperature.
Earlier acceptance artifacts under `/tmp` were lost. Recorded historical results
below remain bounded to their original runs; they do not validate this build.
The [guarded build workflow](DEVELOPMENT.md#guarded-builds) adds resource
containment and persistent monitoring as precautions. The reset's cause remains
unconfirmed, and these precautions are not evidence of a repaired hardware or
kernel fault.

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
