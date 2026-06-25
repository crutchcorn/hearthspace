# Architecture Brief: MCP Window Metadata

To power an infinite canvas desktop shell, the local AI orchestrator must be able to reason about what every open window actually contains ("group all the windows that are research project related").

To do that, Hearthspace exposes window state to a **Model Context Protocol (MCP)** server. The orchestrator queries that server to get a real-time view of each window on the canvas.

All metadata in this system is gathered **dynamically, on demand**. There is no cache, no vector store, and no background ingestion. When the orchestrator asks about a window, Hearthspace queries it live at that moment. The activity/suspension state of a window is explicitly **out of scope** here — that is handled by the separate Window Activity Status system (see [WINDOW_ACTIVITY_STATUS.md](WINDOW_ACTIVITY_STATUS.md)).

---

## 1. The Information Tiers

When the orchestrator requests metadata for a window, Hearthspace gathers it from the following sources. These are layered by richness: higher tiers give better semantic context but are not always available, so lower tiers act as a guaranteed baseline. All of them are queried live.

* **Tier 0: Model Context Protocol (MCP) Broadcast**
  * **Mechanism:** Supported applications explicitly push contextual state (e.g., "Editing index.js") directly to the shell.
  * **Availability:** Best case. Only apps that opt in to the protocol provide this.

* **Tier 1: AT-SPI Accessibility Tree Scrape**
  * **Mechanism:** Query the window's AT-SPI accessibility tree to extract structural text (buttons, paragraphs, labels).
  * **Availability:** Best-effort. AT-SPI is not widely adopted across Linux apps, so this tier is frequently empty or partial. When it is populated it is computationally cheap and fast enough to run on demand.

* **Tier 2: Compositor Metadata**
  * **Mechanism:** Extract Wayland protocol properties (`xdg_toplevel.title`, `app_id`).
  * **Expected Data:** High-level context ("Figma - Landing Page Design"). Always available.

* **Tier 3: Process Descriptor Inspection**
  * **Mechanism:** Scan the kernel's `/proc/[pid]/fd` for the window's underlying process.
  * **Expected Data:** Open file handles (e.g., `/home/user/Documents/Draft.md`), providing context when the higher tiers reveal little.

### Why no OCR

An earlier design included a tier that screenshotted the window framebuffer and ran it through a local OCR engine (e.g., Tesseract). We tried it and the precision was unacceptable for semantic reasoning, so OCR is **deliberately excluded** from this system.

---

## 2. Query Execution (The Recall)

When the user issues a prompt, the orchestrator queries the MCP server for the windows it cares about, merges the live tier data for each, and runs the semantic search against the prompt. Matching windows are then animated to a new cluster on the infinite canvas.

Because everything is live, there is no freshness/staleness concern and no need to distinguish "cached" from "dynamic" windows — every query reflects the current state of every window.
