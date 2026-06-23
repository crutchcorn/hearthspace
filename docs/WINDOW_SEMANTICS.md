# Architecture Brief: Continuous Semantic Metadata Orchestration

**Author:** Corbin Crutchley

This document outlines the ideal state for the app, not how things have landed today. When this body of work is done, let's update it to "This is how it works" language.

## 1. The Architectural Objective

To power an infinite canvas desktop shell, users must be able to semantically query their workspace ("group all the windows that are research project related") at any arbitrary point in time. 

This requires maintaining a real-time semantic footprint for every open window on the canvas, completely independent of the window's suspension state. Because Linux accessibility reporting (AT-SPI) is fragmented, the orchestrator must dynamically route between live querying and cached fallback ingestion.

---

## 2. The Window Initialization Flow

When a new window is spawned and mapped by the compositor, the shell executes an initialization sequence to determine its semantic routing path:

1. **Initialization Delay:** Wait `X` milliseconds to allow the application's internal UI toolkits to fully initialize and mount.
2. **AT-SPI Handshake:** Probe the application for AT-SPI accessibility tree support.
3. **Branching Strategy:**
   * **Path A (AT-SPI Supported):** Mark the window's ID as `Dynamic`. Do not cache its contents. AT-SPI queries are computationally cheap and fast enough to be executed on-demand whenever the local AI orchestrator requires them.
   * **Path B (Unsupported/Opaque):** Mark the window's ID as `Cached`. Immediately execute the Tiered Fallback Pipeline (Section 4) to establish an initial semantic baseline. Store this data in the local vector database and flag the window for periodic background updates.

---

## 3. The Idle-Update Daemon (Cache Maintenance)

For windows routed to **Path B**, their semantic cache must be kept reasonably fresh without exhausting system resources. This is handled by the Wayland-native `idle-culling daemon`.

The daemon tracks input events and surface damage. It introduces a dedicated metadata update threshold:
* **The `N-Minute` Threshold:** When a `Cached` window reaches `N` minutes of inactivity, the daemon triggers the Tiered Fallback Pipeline in the background, updating the vector store with the new window state.
* **Separation of Concerns:** This metadata update occurs entirely independently of—and typically well before—any `Y-Minute` threshold designed to trigger aggressive process suspension (`SIGSTOP`).

Current implementation status:
* Hearthspace has a standalone timer-based per-window idle daemon in `src/idle.rs`.
* Normal app windows are tracked individually by Hearthspace window ID; shell UI is excluded.
* Idle levels are configured by `WINDOW_IDLE_THRESHOLDS` in `src/config.rs`.
* Each level is measured from the previous level transition, not from original window creation.
* Client input and client surface commits reset app-window idle state.
* Compositor chrome interactions, such as title-bar drags and close-button clicks, do not count as app activity.
* The daemon emits transition hooks; today they are logged, and later they should trigger the Tiered Fallback Pipeline for cached windows.

---

## 4. The Tiered Fallback Pipeline

When a window falls into **Path B** (or fails an expected AT-SPI query), the daemon cascades through the following ingestion tiers to guarantee the local AI (Qwen 3.5 4B) always has a valid semantic embedding.

* **Tier 0: Model Context Protocol (MCP) Broadcast (Dynamic)**
  * **Mechanism:** Supported applications explicitly push contextual state (e.g., "Editing index.js") directly to the shell.

* **Tier 1: AT-SPI Accessibility Tree Scrape (Dynamic)**
  * **Mechanism:** Query the active window to extract structural text (buttons, paragraphs). Used primarily for Path A, but acts as the first check here in case an app's accessibility tree suddenly populates.

* **Tier 2: Compositor Metadata (Cached)**
  * **Mechanism:** Extract Wayland protocol properties (`xdg_toplevel.title`, `app_id`).
  * **Expected Data:** High-level context ("Figma - Landing Page Design").

* **Tier 3: Lightweight Background OCR (Cached)**
  * **Mechanism:** The compositor silently grabs a framebuffer screenshot of the window and pipes it through a fast, local OCR engine (e.g., Tesseract).
  * **Expected Data:** An unformatted raw text blob of visually rendered words.

* **Tier 4: Process Descriptor Inspection (Cached)**
  * **Mechanism:** Scan the kernel's `/proc/[pid]/fd` for the window's underlying process.
  * **Expected Data:** Open file handles (e.g., `/home/user/Documents/Draft.md`), providing context when visual scraping fails entirely.

---

## 5. Query Execution (The Recall)

When the user issues a prompt, the orchestrator acts as a map-reduce node:
1. It instantly fires AT-SPI queries for all `Dynamic` windows to get their live text state.
2. It retrieves the embedded states of all `Cached` windows from the local vector store.
3. It merges the datasets and runs the semantic search against the prompt. 
4. Matching windows (whether live or currently suspended) are animated to a new cluster on the infinite canvas.
