# Architecture cleanup

Approved scope: all findings from the `login-toolchain` architecture review,
including persistence/TOML and supporting build/documentation work. The working
branch is `cleanup`, created from `login-toolchain` at
`5de99db2cb11d1f0494c7178a9bb7b050f74994a` on 2026-10-23.

Internal structures and wire formats may change. Preserve the existing visible
behavior unless a listed bug requires correction. This is a working tracker;
source readiness does not mean validation is complete.

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
| C01 | One owned-process cleanup barrier | Implementing |
| C02 | Atomic asynchronous display preference transactions | Implementing |
| C03 | Live output observations separate from preferences | Planned |
| C04 | Durable frame targets with live window bindings | Implementing |
| C05 | One bounded app/file activation path | Planned |
| C06 | Consistent ordinary-app environment | Planned |
| C07 | Shared typed control contracts and bounded framing | Planned |
| C08 | Shared desktop services and UI primitives | Planned |
| C09 | Stable window identity and centralized retirement | Planned |
| C10 | Explicit interaction admission and transitions | Planned |
| C11 | One sampled presentation projection | Planned |
| C12 | Authoritative runtime-artifact inventory | Planned |
| C13 | Valid versioned TOML and legacy migration | Implementing |
| C14 | Cross-process favorites transaction locking | Implemented; focused checks passed |
| C15 | Committed preference refresh/notification ownership | Planned |
| C16 | Documentation and full validation | In progress |

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
persisted success followed by failed compositor reload.

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
