# Window groups

Use **Select window group** in the system tray, or **Super+G**, to select open
windows on the canvas. Click windows to toggle membership, or drag across the
canvas to select a region. **Tab** and **Shift+Tab** choose a window with the
keyboard; **Space** toggles its selection. **Enter** finishes selection, **G**
gathers while selecting, and **Escape** clears the group and cancels any pending
gathering.

The group has a floating toolbar. Drag its handle to move the selected windows
together while preserving their relative positions. The overlapping-window
button, **Gather similar windows**, searches other open windows for related
titles. A moving highlight shows that the decision model is working. Related
windows fly from their original locations to staggered positions on the current
output and join the group. Groups are temporary canvas selections, not saved
workspaces.

## Optional local model

Open **Settings → AI** to download or delete the D1 model, inspect its status,
and choose **Automatic (GPU if available)** or **CPU only**. Ollama must already
be installed and running with support for `POST /v1/systemone`; ordinary chat
support alone is insufficient. An unavailable runtime or missing model produces
an actionable error without freezing window interaction.

Installation uses Rust's `hf-hub` library to download
[`LiquidAI/d1-3B-GGUF/d1-3B-Q4_K_M.gguf`](https://huggingface.co/LiquidAI/d1-3B-GGUF/blob/main/d1-3B-Q4_K_M.gguf)
and registers a separate `hearthspace-d1:latest` model in Ollama. Downloading is
always explicit. Login and gathering never download a model. The downloaded
model lives under `$XDG_CACHE_HOME/hearthspace/ai` (normally
`~/.cache/hearthspace/ai`), and Ollama keeps its own imported copy. Delete removes
Hearthspace's model and cache without deleting other Ollama models.

Once installed, the session warms D1 after desktop startup and keeps it ready
while logged in, renewing a five-minute warm lease every two minutes. Normal
logout requests unloading; the lease also expires after an interrupted session.
Loading and inference run on background workers. The selected
processing mode is retained for later sessions. The model supports a 32,768-token
context; this feature submits only window titles, with bounded input sizes.
Memory and latency depend on the hardware, context, quantization, and runtime;
the model size is not a fixed total-memory guarantee.

## Decisions and cancellation

All original selected window titles become the request's `state`. For each
candidate outside the group, D1 receives that candidate's title in a relevance
question. Every candidate is compared against the same original state, so early
matches do not gradually change the subject of the group. The response is a
typed decision probability, not generated chat text. See the
[D1 model card](https://huggingface.co/LiquidAI/d1-3B-GGUF) for the decision API.

Only the local Ollama service receives titles. No screenshots, window content,
clipboard data, or titles are sent to Hugging Face. The default runtime address
is `http://127.0.0.1:11434`; `HEARTHSPACE_OLLAMA_URL` can select another local
loopback HTTP address. Proxies, remote addresses, and redirects are disabled for
decision requests.

Clearing or changing the selection invalidates an in-progress result. Closed
windows and changed selected titles are checked again before applying results.
Locking the session clears the group. A single worker bounds inference
concurrency; repeated cancellation replaces queued work instead of creating
more inference threads. Requests have deadlines, and a failed gathering leaves
the original group in place. Results wait for an active move or resize to finish
before moving windows, so gathering does not fight a drag in progress.

## Development

The compositor owns selection, input routing, group geometry, and animations.
The shell owns the tray control; settings owns model management. The
`hearthspace-ai` crate contains the local decision client and optional model
download/import lifecycle. Smithay still owns Wayland window and surface
handling. Grouping is compositor-local policy; it requires no client protocol
extension.

The private control socket accepts `group-selection-toggle`, `group-clear`,
`group-gather`, and `group-status`. Status includes selected IDs, selection and
gathering flags, errors, and the floating toolbar's logical bounds. These
commands follow the same session-lock restrictions as other desktop commands.
