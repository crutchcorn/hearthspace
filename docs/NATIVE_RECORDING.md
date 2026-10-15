# Recording a native demo

Use the [native control workflow](../.agents/skills/hearthspace-native-control/SKILL.md)
to own a spare VT and restore the original session afterward. Install the optional
[FFmpeg dependency](../CONTRIBUTING.md#native-screen-recordings) first.

Start a fresh session with `--isolated-profile` so the default dotted background
and empty application profiles appear in the recording. Set the display scale to
at least 2× for readable shell and application text. Use an optimized native build
for recording; build it before taking the VT. The lower-level launcher accepts
`--no-build --binary-dir /absolute/path/to/target/release` after that build passes.

In a separate terminal, record the explicitly owned session:

```sh
scripts/hearthspace-record --session /absolute/native-run \
  --output /absolute/demo.mp4 --duration 60 --width 1600
```

The output must be new. The recorder announces readiness after its first frame,
then captures until the duration expires, Ctrl+C is pressed, or the optional
`--stop-file /absolute/new-stop-file` appears. It does not start or stop the
desktop. Stop the native session through its launcher when filming finishes and
verify `cleanup.complete` and restoration of the original VT.

The MP4 contains actual native captures with the cursor. PNG frames, capture
metadata, timestamps, encoder logs, and binary provenance remain alongside it in
`demo.recording/`. The video uses elapsed capture time, including pauses, and is
encoded as H.264/yuv420p at 30 fps with fast-start metadata. Distinct capture
cadence depends on the host; inspect `recording.json` rather than assuming every
encoded frame is a new presentation. Timestamps are command-response receipt
times, not hardware presentation timestamps. Keep geometry and scaling fixed
throughout capture. Locking or changing outputs aborts the recording.

For a human-paced introduction, allow roughly 4 seconds for the empty canvas,
20–25 seconds to launch and arrange three apps, 12–15 seconds to navigate in four
directions, and 5–8 seconds for the minimap. Move the cursor deliberately, pause
after each action, and keep app content free of personal information. Review the
whole sequence at playback size before publishing; inspect opening, launch,
movement, navigation, and minimap frames and verify that the file decodes cleanly.

Replace `assets/demo.mp4` with the reviewed recording. To retain GitHub's inline
player in README, upload it as an attachment using GitHub CLI 2.99 or newer
(`gh pr edit NUMBER --attach assets/demo.mp4`) and use the resulting
`https://github.com/user-attachments/assets/...` URL. Preserve the PR description
when uploading and retain the attachment URL in later description updates.
