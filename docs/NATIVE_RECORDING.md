# Recording a native demo

The [README demonstration](../assets/demo.mp4) should show what using Hearthspace
feels like: arrange a few real applications, move between areas of work, find a
window, and personalize the desktop. Record the current native interface with
public sample content. A demo is an introduction, not a substitute for the
[acceptance checks](DAILY_SMOKE.md).

## Prepare the session

Follow the [native control workflow](../.agents/skills/hearthspace-native-control/SKILL.md)
to own a spare VT and restore the original session afterward. Install the
optional [recording dependencies](DEPENDENCIES.md) first. Build the optimized
runtime before taking the VT; [development workflows](DEVELOPMENT.md) explain
the shared binary inventory. The native launcher accepts
`--no-build --binary-dir /absolute/path/to/target/release` after that build passes.

Start a fresh session with `--isolated-profile`. Use sample documents without
personal information, and avoid recording credentials, private notifications,
or other desktops. Set a readable display scale before recording and keep the
output geometry fixed. Prepare the applications and camera position before the
opening frame; let the viewer see their arrangement before moving anything.

## Capture actual interaction

In a separate terminal, record the explicitly owned session:

```sh
scripts/hearthspace-record --session /absolute/native-run \
  --output /absolute/demo.mp4 --duration 90 --width 1600
```

The output must be new. The recorder announces readiness after its first frame,
then captures until the duration expires, Ctrl+C is pressed, or the optional
`--stop-file /absolute/new-stop-file` appears. It does not start or stop the
desktop. Encoding continues after capture stops; allow it to finish before
terminating the recording process. Stop the native session through its launcher
and verify `cleanup.complete` and restoration of the original VT.

The MP4 contains actual native captures with the cursor. PNG frames, capture
metadata, timestamps, encoder logs, and binary provenance remain alongside it in
`demo.recording/`. The video uses elapsed capture time, including pauses, and is
encoded as H.264/yuv420p at 30 fps with fast-start metadata. Distinct capture
cadence depends on the host; inspect `recording.json` rather than assuming every
encoded frame is a new presentation. Timestamps are command-response receipt
times, not hardware presentation timestamps. Locking or changing outputs aborts
the recording.

A useful sequence takes about a minute: pause on the workspace, move a window,
navigate the canvas, show the map and search, switch applications, and change an
appearance setting. Leave a few seconds after each change. If showing a power
confirmation, cancel it; an introduction does not need a shutdown or real sleep.
Keep cursor movement deliberate and avoid speeding up the footage to hide stalls.

## Review and replace the demo

Watch the whole sequence at normal playback speed and the size readers will see.
Check readable text, cursor visibility, clean transitions, accurate timing, and
that each intended interaction actually happened. Decode the complete file:

```sh
ffmpeg -v error -i /absolute/demo.mp4 -f null -
```

Replace `assets/demo.mp4` only after that review. Extract a representative frame
for `assets/demo-poster.png`, and keep README's poster linked to the checked-in
video. No external upload or pull-request comment is needed for that link.
Retain raw footage, provenance, and cleanup evidence outside the repository;
commit the finished video and poster, not a machine-specific capture journal.

The current demonstration shows window arrangement, canvas navigation and map
zoom, search, switching, light/dark appearance, accent selection, and a cancelled
Log off confirmation. Its sample documents contain no personal work.
