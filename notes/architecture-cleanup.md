# Architecture cleanup

Approved scope: all findings from the `login-toolchain` architecture review,
including persistence/TOML and supporting build/documentation work. The working
branch is `cleanup`, created from `login-toolchain` at
`5de99db2cb11d1f0494c7178a9bb7b050f74994a` on 2026-10-23.

Internal structures and wire formats may change. Preserve the existing visible
behavior unless a listed bug requires correction. All 16 items are implemented
and validated as of 2026-10-23. The item descriptions retain the approved review
scope; the evidence log is chronological, with final results below. Maintained
boundaries and migration guidance live in [Architecture](../docs/ARCHITECTURE.md).

## Boundaries to retain

- `App` remains the Smithay protocol/seat integration root. Smithay grabs own
  input sequences; cancellation must drain held releases.
- Rich UI remains ordinary Wayland clients. Pure geometry stays independent of
  clients, rendering, and persistence.
- Canvas homes are canonical placement; animations and previews are presentation.
- Keep feature-specific workers and synchronization. A generic scene, event,
  plugin, or state-sync framework is not required.
- Session ownership uses UID, PID/start time, generation, inode, and unit checks.
  Borrowed user services are never adopted for termination.
- Preserve installed immutable releases, isolated development profiles, explicit
  audio overrides, Snap/Ptyxis adapters, and host activation ownership restoration.

## Item index

| ID | Cleanup | Status |
| --- | --- | --- |
| C01 | One owned-process cleanup barrier | Complete |
| C02 | Atomic asynchronous display preference transactions | Complete |
| C03 | Live output observations separate from preferences | Complete |
| C04 | Durable frame targets with live window bindings | Complete |
| C05 | One bounded app/file activation path | Complete |
| C06 | Consistent ordinary-app environment | Complete |
| C07 | Shared typed control contracts and bounded framing | Complete |
| C08 | Shared desktop services and UI primitives | Complete |
| C09 | Stable window identity and centralized retirement | Complete |
| C10 | Explicit interaction admission and transitions | Complete |
| C11 | One sampled presentation projection | Complete |
| C12 | Authoritative runtime-artifact inventory | Complete |
| C13 | Valid versioned TOML and legacy migration | Complete |
| C14 | Cross-process favorites transaction locking | Complete |
| C15 | Committed preference refresh/notification ownership | Complete |
| C16 | Documentation and full validation | Complete |

## C01: One owned-process cleanup barrier

The installed login guard waits for the logind session scope, but can remove the
runtime before the independent app-scope guard finishes stopping user-manager
app scopes. Normal supervisor shutdown already waits. This establishes a race,
not evidence of permanently leaked applications.

Extract the existing bounded `AppScopes` stop/wait operation for supervisor and
installed cleanup-helper use. Run it before unmount/removal, including persisted
startup intent without an in-memory owner. Keep independent watchdogs; never
make the owner guard wait for its own termination. Preserve activation generation
retirement and retain runtime/recovery evidence on failure.

Acceptance: delayed owned-scope exit, missing owner, failure/timeout, supervisor
death, and unrelated-service survival. Login cleanup must not unmount or delete
before the barrier succeeds.

## C02: Atomic asynchronous display preference transactions

Settings currently writes scale and each position separately. A later failure
leaves earlier writes committed. Newly discovered connector IDs are displayed
but setters only recognize existing saved IDs.

Introduce a typed whole-layout operation, validate before any write, upsert new
identities, and retain disconnected preferences. Submit through a serialized UI
worker with edit generations. Completion merges only owned display fields;
appearance/background changes and newer local previews must survive.

Acceptance: first save of a third display, invalid transaction leaves the whole
file unchanged, concurrent edits, local preview/selection preservation, and
successful persistence while the compositor is unavailable.

## C03: Live output observations separate from preferences

Settings probes Wayland once, while the shell already continuously observes
outputs. Settings and compositor also resolve legacy primary/secondary roles
using different ordering rules.

Extract the existing observer into a UI-independent client module, share pure
connector/legacy-role resolution, and keep connected observations distinct from
persisted preferences. Adapt the open Displays page on hotplug without replacing
pending local edits or discarding disconnected settings.

Acceptance: reordered enumeration, hotplug and mode changes while open, selected
output removal, pending edit completion, and stable shell per-output identities.

## C04: Durable frame targets with live window bindings

Live layout currently replaces saved app-slot IDs with runtime IDs. After an
unexpected destruction, the next reconciliation drops the claim; another pass
then drops the saved attachment. Disk loss requires a later successful save.
Ordinary Wayland unmap retains its record and is different from destruction.

Keep saved app-slot identities canonical and resolve live bindings only for
placement/status/surface lookup. Numeric transient targets remain runtime-only.
Explicit confirmed close still removes its preference. Release bindings on actual
record removal, not reversible Wayland unmap.

Acceptance: multiple reconciliations after disappearance, replacement window IDs,
grouped attachments, X11 unmap/destruction, invisible absent slots, explicit
confirmed-close deletion, and restoration without viewport movement.

## C05: One bounded app/file activation path

Ordinary launches synchronously parse/expand `Exec` and prepare resources in the
compositor, while file launching has GIO activation and a bounded worker. The
catalog rejects entries without `Exec` and does not retain `DBusActivatable`.

Extend the existing GIO/worker path to ordinary activation. Keep catalog matching,
terminal selection, Settings arguments, Snap adapters, literal arguments, and
working directories. Final display/output registration and lock checks stay on
the compositor thread.

Acceptance: cold/warm activation, errors, activation-only entries, terminal apps,
Settings links, literal filenames, output targeting, and cancellation before
launch. No blocking resource preparation in the compositor dispatch loop.

## C06: Consistent ordinary-app environment

Direct native children inherit a private runtime/relative display; shared-bus
activation substitutes the host runtime/absolute display and omits live X11
state. Derive one native ordinary-app environment for both routes. Keep private
Hearthspace sockets and records in the session directory and preserve explicitly
isolated harness policy.

Acceptance: both routes reach the same display, bus/keyring, audio, and portals;
Snap/Ptyxis ownership and explicit overrides survive; private profiles are never
published into the shared user manager.

## C07: Shared typed control contracts and bounded framing

The compositor imports commands from shell implementation; layout requests and
window metadata are independently encoded/parsed. Shell and Settings duplicate
the `ok <length>` payload reader.

Move shared request/response types and codecs to feature-specific IPC modules.
One bounded transport reader accepts caller limits/deadlines. Keep UI labels and
execution local. Preserve distinct canvas revision and popup acknowledgment
semantics rather than introducing a generic synchronizer.

Acceptance: truncated/malformed/oversized frames, session/token validation,
acknowledgment ordering, and real producer-consumer integration.

## C08: Shared desktop services and UI primitives

Settings/compositor reuse catalog, favorites, file activation, and artwork through
the shell package. General artwork depends on tray identity. `GesturePad` already
has generic state/callbacks and two consumers.

Extract the reused non-UI desktop services into one library with a neutral raster
type. Move the reusable gesture primitive/theme/thumbnail support to existing
UI components. Keep shell desktop enumeration/context orchestration and workers
local. The combined executable may retain its intentional `--shell` dependency.

Acceptance: desktop-entry precedence, launch environment ownership, favorites
validation, bounded asynchronous decoding, stale gesture rejection, and Settings
recording remaining incapable of launching applications.

## C09: Stable window identity and centralized retirement

The mutable window vector mixes identity with stacking indices. Raising removes
and reinserts records; XDG unmap/destroy and X11 removal duplicate cleanup.

Encapsulate storage/ID issuance/stack mutation in `windows::State`, use stable IDs
across feature operations, and centralize retirement with explicit causes. Keep
protocol mapping rules in adapters and indices local to synchronous traversal.

Acceptance: family stacking, reversible remap, override-redirect handling, focus
recovery without panning, and unrelated active operations surviving teardown.

## C10: Explicit interaction admission and transitions

Navigation, pinch, touch, search, switcher, and reset use scattered overlapping
conditions/cancellation sequences. Different permissions are sometimes necessary.

Introduce a compositor-local action/source/phase policy and named transitions,
initially reusing established navigation checks. Feature state machines and
Smithay grabs remain authoritative.

Acceptance: owned sequences continue/drain correctly through cancellation,
destruction, extra contacts, focused-view switching, and local/remote input.

## C11: One sampled presentation projection

Preview helpers independently sample the clock for origin, bounds, scale, and
inverse transforms. Return one immutable projection with origin/bounds/scale/
opacity sampled at one time; reuse existing motion and presented-hit handling.

Acceptance: fixed-time transform round trips, rendered/input bounds agreement,
insets/decorations, fractional zoom, output-pinned dialogs, preview exit, reduced
motion, and unchanged canonical canvas homes.

## C12: Authoritative runtime-artifact inventory

Development startup assumes a built sibling scope guard, installer and harness
maintain separate binary inventories, and `dev-run-app` ignores a custom Cargo
target directory when locating the result.

Share a concrete package/binary inventory. Development bootstrap prepares helpers
before acquiring resources; resolve artifacts through Cargo consistently. Keep
intentional separate feature builds and unprivileged compilation.

Acceptance: clean checkout, custom target directory, fresh helper provenance,
native features, literal Settings arguments, build failure before VT acquisition,
and checkout-independent installed execution.

## C13: Valid versioned TOML and legacy migration

The handwritten parser truncates values at `#` and the formatter writes unquoted
strings. Use the existing TOML ecosystem and a versioned serde document. Read the
legacy form for migration, write valid TOML atomically only after successful
validation, and preserve malformed/unknown-version files instead of overwriting
them with defaults.

Acceptance: legacy migration, quotes/hash/newline/Unicode round trips, unsupported
versions, malformed files, and transactional persistence failures.

## C14: Cross-process favorites transaction locking

Two read/compare/rename writers can both pass the optimistic check and overwrite
one another. Hold a persistent sidecar advisory lock across compare and commit;
bound acquisition and preserve stale-writer errors and in-memory rollback.

Acceptance: actual competing processes, no lost successful update, stale conflict,
lock timeout/recovery, private files, and no leftover temporary files.

## C15: Committed preference refresh/notification ownership

Preference writers, compositor reload commands, and independently polling UIs
currently divide refresh responsibilities. Give committed changes one explicit
revision/notification contract at the existing persistence boundary; consumers
refresh owned projections without overwriting local pending edits. Keep backing
store ownership explicit when sessions share a profile.

Acceptance: every consumer sees acknowledged commits, failed writes publish no
change, concurrent appearance/display updates survive, and reconnect refreshes.

## C16: Documentation and validation gate

Update maintained guides as boundaries land, including migration behavior,
runtime artifacts, and any required host changes in `CONTRIBUTING.md`. Keep this
tracker honest about remaining work and evidence.

Required final checks: workspace formatting, Ruff check/format, full Clippy and
build, workspace/all-target tests with `udev,e2e,test-apps`, Python regressions,
and native acceptance for changed cross-process/display/input behavior using the
repository harness. Run resource-heavy checks serially. Review important
lifecycle/security boundaries independently. Commit validated increments; no
push is implied by this task.

## Implementation and evidence log
- 2026-10-23: fetched `login-toolchain`, confirmed reviewed SHA, created `cleanup`.
  C01/C02/C04/C13 implementation and focused regressions are in progress. No
  execution validation or completion claim yet.
- C14: added a stable private advisory lock spanning comparison through rename,
  bounded at 500 ms. The 33 focused favorites tests pass, including competing
  subprocesses and timeout/retry. Shell library/test Clippy passed with warnings
  denied. Full workspace and subsequent extraction validation remain required.
- C01: the installed cleanup helper and supervisor now share the persisted-intent
  app-scope barrier. Focused Rust helper/supervisor tests and all 25 login-guard
  Python tests pass. Native scope-order acceptance is still pending.
- C02/C13: 174 IPC, Settings, and daemon tests passed after atomic display
  transactions and real TOML migration. C15 extends these files; rerun required.
- C03: common 97, Settings 131, and Wayland observer 14 tests passed. A subsequent
  position-observation addition had a dispatch-arm compile error, corrected by
  root; its final rerun remains pending. No native hotplug evidence yet.
- C07/C08: shared desktop services passed 110 unit and six integration tests;
  UI components passed 48. Typed IPC contracts passed 29 tests, and the shell
  library passed 430. New C05/C15 integrations require subsequent reruns.
- C12: session binary 120 tests passed (one existing ignored), plus seven artifact
  preparation regressions. Four inventory and eight dev-app Python tests passed.
  Full Python suite passed 586 tests, with 14 skips; no host installation occurred.
- C04/C09/C10/C11: durable bindings, window store/retirement, interaction policy,
  and sampled projection source is ready. Compositor compile/headless validation
  and independent lifecycle/projection review remain required.
- C15: 33 IPC, 135 Settings, and 41 daemon tests passed. Picker cancellation and
  failure regressions were added afterward; compositor subscription and consumer
  reruns remain pending. The idle-X11 integration now requires committed changes
  without sending a reload command.
- C05/C06: source is ready and desktop tests passed. Integrated session checks
  exposed non-private fixture directories under umask 0002 and asynchronous portal
  name release; fixtures and explicit release have been corrected, rerun pending.
- C10/C11 independent review found no policy issue. Two preview gaps are being
  repaired: modal hit selection must use the saved render sample, and the terminal
  animation sample must schedule its final redraw even after the wall-time deadline.
- Integrated validation: all 514 compositor library tests passed, including the
  ten cached-preview/final-frame regressions. Real-client runs passed 26 grab
  lifecycle cases, 12 X11 interaction cases, and the new family/remap/ID case.
  The frame filter passed 35/36: the remaining fixture overlaps the default
  system group during intentional ungrouping; a separate canonical drop review
  finding is being repaired with a regression.
- Settings review repairs now pass all 141 Settings and 42 daemon tests: retained
  disconnected drafts, independent primary-metadata retry, equality/generation
  acknowledgment ordering, and both response-encoding limits. The shared
  observer's 14 tests pass. Independent repair review is running.
- Integrated session rerun passed 129 tests (one pre-existing ignored). The
  launch/environment review identified inherited nested activation ownership,
  complementary audio overrides, and launch queuing behind desktop filesystem
  work; repairs and five asynchronous UI fixture migrations are in progress.
- Python Ruff check and format checks pass for all 195 files. Full workspace
  checks, native acceptance, documentation finalization, and commits remain.
- C04 repair: all 38 frame-layout real-client tests pass. Deliberate drops resolve
  canonical slots before settlement; absent and transient targets retain their
  intended behavior. Independent review found no remaining issue.
- C10/C11: focused real-client suites pass: canvas settlement 15, canvas search
  11, switcher 7, remote input 4, CSD lifecycle 7, and session locking 5. The
  preview repair review found no remaining issue.
- C15: idle X11 appearance updates now pass without a reload command. Display
  acknowledgment review requires post-commit reconciliation for ambiguous
  observations and generation advancement even on identical reconnect reads;
  the final narrow correction remains under validation.
- C01/C12 independent source review passed. The activation-failure, eight cleanup
  barrier, two scope-guard, and seven artifact-preparation tests pass after test
  directories were made explicitly private instead of depending on umask.
- Recorded `2ce341d` for shared IPC and transactional settings, and `298a4ad` for
  output observation/identity. Package Clippy with warnings denied and formatting
  passed for these boundaries. No push performed.
- C05/C06 integration: desktop 118 unit plus six integration tests, session 131
  unit tests (one pre-existing ignored), and shell 436 library tests pass. Nested
  selected-bus inheritance and caller-lifetime cancellation during preparation
  remain the final launch review repairs; audio publication/retirement passed
  both real broker/manager tests and source review.
- Final launch lifetime review found no remaining issue. Caller deadlines and
  cancellation reach pre-spawn admission; legacy half-close/capture and internal
  launch semantics remain intact. Settings now passes all 145 tests, including
  same-cursor reconnect reconciliation. Workspace rustfmt, Ruff, and Clippy with
  warnings denied pass; the full build and execution suites are running.
- Final workspace gate passes formatting, Ruff, warning-free Clippy, full build,
  and 1,986 Rust tests across 52 suites (one existing isolated helper ignored).
  The first full Python run executed 586 tests and found two failures: obsolete
  missing-program error wording and pending launch feedback shifting the favorite
  drawing pad. The error assertion now checks the preserved ENOENT diagnostic.
  Pending drawing feedback uses its reserved slot; both rendered regressions
  fail with the original row and pass with the repair. Real workflow and full
  Python reruns are in progress. Evidence is retained under
  `/tmp/hearthspace-cleanup-final-e2e` and `/tmp/hearthspace-cleanup-final-e2e-2`.
- The focused real favorite workflow passes, including cancelled/unknown drawings
  and scaled immediate launch. The following full Python run passed its other
  585 tests but correctly rejected reuse of that focused run's evidence directory.
  No product change was needed; the complete suite is rerunning in a fresh
  `/tmp/hearthspace-cleanup-final-e2e-3` directory. Implementation is committed
  as `e494532`; native acceptance and final documentation are still pending.
- Final Python run passes all 586 tests with E2E/accessibility/tray requirements
  enabled and no skips. Native presentation and shell workflows pass; the
  switcher passes all 41 keyboard/touchpad/touchscreen stages. Each session
  restored tty2, stopped owned scopes, and removed its runtime.
- The exclusive-login-guard crash fixture was inconclusive: its allocation-lock
  lease also delayed the native stop path. The child exited while runtime remained;
  eventual recovery completed, but that does not prove which cleanup owner removed
  the directory. Preserve attempt `hearthspace-cleanup-native-barrier-acceptance-20261023-1`.
  The separate shared-helper and ordinary crash runs below replace this test
  strategy. The failed attempt remains preserved; no production repair was needed.

## Final validation

All implementation and review repairs are committed through `e494532`; maintained
architecture/migration documentation is committed as `d6a4d41`. No push occurred.

- Workspace rustfmt, Ruff check/format, warning-free Clippy, and full build pass.
  Cargo checks used `--locked --workspace --all-targets` and
  `--features udev,e2e,test-apps`; Clippy used `-D warnings`.
- Rust: **1,986 passed**, zero failed, one existing isolated helper ignored across
  52 suites. [Rust/build/lint evidence](/tmp/hearthspace-cleanup-final-e2e-2).
- Python: **586 passed**, zero skipped, with E2E, accessibility, and tray checks
  required. [Fresh full-run evidence](/tmp/hearthspace-cleanup-final-e2e-3).
- Native presentation, shell/Settings, and 41 switcher input stages pass:
  [presentation](/tmp/hearthspace-cleanup-native-success-20261023-1),
  [shell](/tmp/hearthspace-cleanup-native-shell-20261023-1), and
  [switcher](/tmp/hearthspace-cleanup-native-switcher-20261023-1).
- The [installed shared helper](/tmp/hearthspace-cleanup-native-live-helper-20261023-1)
  remained pending after the delayed app exited, then succeeded 54 ms later.
  The original runtime and supervisor survived until normal harness stop.
- [Composed crash recovery](/tmp/hearthspace-cleanup-native-crash-20261023-1)
  positively observed app exit with the original runtime still present, then
  removal 18 ms later and matching `login-cleaned`. Both cases retained every
  child runtime check, preserved the unrelated sentinel, and cleaned all owned
  processes/runtime with tty2 restored. They do not claim exclusive guard removal;
  actual-guard deterministic tests establish its call-before-delete boundary.
- Hardware: Linux `7.0.0-38-generic`, physical AMD Strix Radeon 880M/890M,
  `amdgpu`, one 2560×1600 eDP output; native captures cover scales 1 and 2.
  Multi-output hotplug is covered by protocol tests, not physical unplugging.
  Crash cases use a private bus; shared activation uses isolated broker tests.
  No host packages or configuration were changed by this cleanup.
