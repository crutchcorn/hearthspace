---
name: web-parity
description: Compare Hearthspace with its live web design reference and bring native visuals and interactions as close to 1:1 as the real desktop permits. Use for visual parity work, porting a reference interaction, reviewing shell or Settings changes, or checking responsive and animated states against the reference.
---

# Web Parity

The live design reference is:

**https://crutchcorn.github.io/hearthspace-ui-experiments/**

Inspect the live page for the task at hand. It changes over time; remembered
screenshots and this skill don't establish its current behavior. If source or
design rationale is needed, read the relevant files through the public repository
at https://github.com/crutchcorn/hearthspace-ui-experiments using its web UI or
remote API. Record the revision used in task evidence.

Do not clone the reference project, search for an existing local checkout, or
record a local reference-project path in this repository. Browser observations,
remote source reads, and temporary comparison captures are sufficient.

## Establish the comparison

Read Hearthspace's relevant maintained feature guide and implementation. Open
the corresponding live reference surface in a browser. Work toward a 1:1 result,
including interaction and motion, rather than matching a single resting image.

Before editing, record a compact state matrix for the affected feature:

| Dimension | What to inspect |
| --- | --- |
| Entry and exit | Trigger, initial focus, outside click, Escape, confirmation, restoration. |
| Geometry | Viewport, scale, surface bounds, padding, gaps, alignment, overflow, anchoring. |
| Appearance | Light/dark, accent, font family/size/weight/line height, icons, strokes, materials. |
| Interaction | Hover, pressed, selected, focused, disabled, loading, empty, error. |
| Layout | Default corners, moved canvas control, each frame edge, grouped and compact forms. |
| Input | Pointer, touch, keyboard navigation, activation, scrolling, cancellation. |
| Motion | Start/intermediate/end poses, easing/duration, interrupted/reversed transitions, reduced motion. |

Choose relevant rows; don't turn a small change into an unrelated feature audit.
Capture both products at comparable logical viewport dimensions and color mode.
Separate device pixels, logical units, output scale, and canvas zoom. A screenshot
at twice the pixel density isn't evidence that the native control is twice as big.

## Measure before approximating

Use the browser's semantic tree and computed styles where available. Inspect
actual bounds, font metrics, border widths, shadows, blur, opacity, SVG paths,
and responsive rules. Observe a transition at more than its endpoints. Keep
capture dimensions and state with the evidence so another contributor can
reproduce a difference.

Map the reference's visual roles onto existing native tokens and components:

- Use `crates/ui-components` for shared controls, theme properties, and glyphs.
- Keep shell/Settings feature state with its existing owner.
- Let the compositor own placement, clipping, backdrop sampling, focus routing,
  input isolation, and presented animation poses.
- Preserve application artwork and third-party chrome; first-party consistency
  doesn't require restyling other apps.

Inspect the current native material and icon implementation before adding a new
widget, renderer, timer, or dependency. Fix the shared cause when several
surfaces differ for the same reason. Don't compensate for a compositor geometry
error with arbitrary client padding.

## Translate behavior into a real desktop

The reference contains simulated apps, device values, and some demonstration
workflows. Use its appearance and interaction intent, then connect to actual
native state. Check whether a feature is implemented before describing it as
available.

Never port demo authentication, fill-password shortcuts, fake capabilities,
simulated downloads, or dismiss-only power actions into production behavior.
Keep the native session-lock boundary, PAM authentication, capability checks,
confirmation gates, caller cancellation, and failure feedback intact. Unsupported
hardware or service state must remain honestly unavailable.

A necessary native difference should be explicit and small: for example, a real
Suspend action, an unavailable radio, a toolkit-owned app interior, or a shortcut
reserved by desktop conventions. Explain the reason in the change review. Don't
use “native” as a reason to accept avoidable spacing, alignment, focus, or motion
differences.

Match behavior across input types. Retain accessible names and focus indicators
when visible copy is removed. Press and release belong to the same target;
rebuilds, capability changes, or modal dismissal must not transfer activation.
Touch must work without hover, including scrolling and cancellation.

## Verify the result

For native interaction, follow
[hearthspace-native-control](../hearthspace-native-control/SKILL.md). Use an owned
session, the private accessibility bus, and presentation-confirmed captures.
Wait for the required semantic control after accessibility activation; a mapped
surface or an Application root alone doesn't establish a ready tree.

Exercise the same state sequence in the reference and native UI. Compare
side-by-side captures at matching logical dimensions, then inspect motion at
normal speed. Check the affected light/dark, narrow, side-edge, and grouped
states, plus keyboard and touch where relevant. A static pixel match doesn't
prove focus restoration, hit bounds, or cancellation.

Add regressions at the actual failing boundary: rendered widget geometry for a
layout shift, real protocol clients for compositor ownership, complete key-down
and key-up phases for activation, or native captures for presentation. Keep
reference measurements as evidence, not a copied implementation test.

Report what now matches, any deliberate native differences, the exact states
checked, and remaining gaps. Update native usage docs with native behavior only;
keep discovery of the web reference in this skill.
