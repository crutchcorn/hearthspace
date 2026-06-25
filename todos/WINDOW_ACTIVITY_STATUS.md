# Architecture Brief: Window Activity Status

On an infinite canvas desktop shell, most windows are off-screen and untouched most of the time. To keep the machine responsive, Hearthspace tracks how long each window has been inactive and escalates how aggressively it reclaims that window's resources as inactivity grows.

This is purely about a window's **activity lifecycle** — how idle it is and what we do about it. It has no overlap with the metadata/semantics work; for that see [MCP_WINDOW_METADATA.md](MCP_WINDOW_METADATA.md).

---

## 1. Current Implementation: The Idle Daemon

Today this system is a rough, timer-based per-window inactivity tracker. It tracks elapsed inactivity and emits transition hooks, but does not yet act on them.

* Hearthspace has a standalone timer-based per-window idle daemon in `src/compositor/idle.rs`.
* Normal app windows are tracked individually by Hearthspace window ID; shell UI is excluded.
* Idle levels are configured by `WINDOW_IDLE_THRESHOLDS` in `src/config.rs`.
* Each level is measured from the previous level transition, not from original window creation.
* Client input and client surface commits reset app-window idle state.
* Compositor chrome interactions, such as title-bar drags and close-button clicks, do not count as app activity.
* The daemon emits transition hooks; today they are only logged.

---

## 2. Future Direction: Tiered Resource Reclamation

The idle thresholds are intended to drive progressively more aggressive reclamation as a window stays inactive. The planned tiers, in order of escalation:

* **Tier 1 — Freeze:** When a window crosses an early inactivity threshold, capture its last-known screenshot to display in place of the live surface, then freeze the window's process via the kernel **cgroup freezer**. The window stops consuming CPU but can be thawed instantly when the user returns to it.
* **Tier 2 — Close:** When a window crosses a later inactivity threshold, fully close it to reclaim all of its resources.

The transition hooks the idle daemon already emits are the intended trigger points for these actions.
