# Window groups

Select related windows to move them together without changing their sizes or
relative spacing. This is a temporary canvas selection; persistent strips at
the screen edge belong to [frame groups](SHELL.md#change-layout).

Use **Select window group** in the bottom-left Canvas tools, or **❖ + G**, then
draw a freeform lasso around windows on the canvas. Windows inside or crossed by the
enclosed shape are selected. Releasing the pointer or lifting your finger
finishes selection immediately; the last point connects to the first. Drawing
a new lasso replaces the previous group. A tap or a line with no enclosed area
finishes without selecting windows.

Drag any selected window, including its content, to move the whole group while
preserving the windows' relative positions. While grouped, these gestures move
windows rather than interacting with their contents. Click or touch outside the
selected windows to clear the group and resume normal interaction. **Escape**
also clears it and cancels any pending gathering.

Hold **Alt** during a group drag to align the grabbed window's visible top-left
corner with the canvas dot grid. Every member moves by the same amount, keeping
their spacing intact. Pressing or releasing Alt updates alignment immediately,
including while the pointer is still.

The floating overlapping-window button beside the group, **Gather similar
windows**, searches other open windows for related titles. Its moving highlight
shows that the decision model is working. Related windows fly from their
original locations to staggered positions on the current output and join the
group. Gathering changes their actual canvas positions, unlike the temporary
previews used by search and window switching. The selection itself is not saved.

Keyboard selection remains available: after **❖ + G**, **Tab** and
**Shift+Tab** choose a window, **Space** toggles its selection, **Enter** finishes,
and **G** gathers while selecting.

## Optional local model

Open **Settings → AI** and turn on **Enable AI features** to download and prepare
the D1 model, or enable an existing installation without downloading again.
Progress and errors appear below the switch. Turning it off opens a confirmation;
**Turn off and delete** removes the model and its download. Gather respects this
enabled state, even when an external installation is available. Ollama must
already be installed and running with support for `POST /v1/systemone`; ordinary chat
support alone is insufficient. An unavailable runtime or missing model produces
an actionable error without freezing window interaction.

Installation uses Rust's `hf-hub` library to download
[`LiquidAI/d1-3B-GGUF/d1-3B-Q4_K_M.gguf`](https://huggingface.co/LiquidAI/d1-3B-GGUF/blob/main/d1-3B-Q4_K_M.gguf)
and registers a separate `hearthspace-d1:latest` model in Ollama. Downloading is
always explicit. Login and gathering never download a model. The downloaded
model lives under `$XDG_CACHE_HOME/hearthspace/ai` (normally
`~/.cache/hearthspace/ai`), and Ollama keeps its own imported copy. Delete removes
Hearthspace's model and cache without deleting other Ollama models. If Ollama is
unavailable during removal, the download cache can be deleted while its imported
copy remains; Settings reports the error so removal can be retried when the runtime
is available.

Once enabled, the session warms D1 after desktop startup and keeps it ready
while logged in, renewing a five-minute warm lease every two minutes. Normal
logout requests unloading; the lease also expires after an interrupted session.
Loading and inference run on background workers. Automatic device selection is
the default; existing device preferences are retained. The integration configures a
32,768-token context; this feature submits only window titles, with bounded input sizes.
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
The shell owns the Canvas tools control; Settings owns model management. The
`hearthspace-ai` crate contains the local decision client and optional model
download/import lifecycle. Smithay still owns Wayland window and surface
handling. Grouping is compositor-local policy; it requires no client protocol
extension.

The private control socket accepts `group-selection-toggle`, `group-clear`,
`group-gather`, and `group-status`. Status includes selected IDs, selection and
gathering flags, errors, and the floating button's logical bounds
(`gather_button`). These commands follow the same session-lock restrictions as
other desktop commands.
