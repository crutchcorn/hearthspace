# Daily usability

## Goal

Make Hearthspace practical for a sustained local work session on this machine's
native TTY: launch and use a browser, editor, terminal, and file manager; find and
manage their windows without losing work; retain ordinary application settings;
and perform the same work through documented agent commands. Session startup,
shutdown, and failures must preserve the background agent and restore the prior
desktop. Completion requires observed workflows, not just available commands.

This builds on [AI enablement](./AI_ENABLEMENT.md), the
[native backend](./NATIVE_BACKEND.md), and the [roadmap](./ROADMAP.md). The planning
review also covered cursor behavior, live window metadata, accessibility, and
window activity. Destructive idle reclamation and speculative AI features do not
help this milestone; retain application data and prioritize reliable interaction.

## Acceptance

- [ ] Keyboard launch, forward/reverse window switching, close, canvas navigation,
      and recovery of off-screen windows work with overlapping everyday apps.
- [ ] Newly mapped apps receive focus; activation matches actual focus; closing a
      focused app returns focus to a useful surviving window.
- [ ] Maximize/fullscreen and restore work on Wayland and XWayland, including a
      zoomed canvas; dialogs and menus remain reachable.
- [ ] Agents discover installed apps and live windows, target stable window IDs,
      enter Unicode text, and receive errors for failed actions. Screenshots and
      semantic state verify each action's intended result.
- [ ] Settings/control clients have bounded I/O. A stalled peer cannot indefinitely
      freeze input, display updates, or subsequent automation.
- [ ] A native session has working audio, ordinary application configuration,
      useful desktop service integration, and a clear way to exit safely.
- [ ] Frequently needed system interactions (notifications, files, links,
      permissions, and session protection) have working, documented paths.
- [ ] Useful spatial state survives a normal restart without automatically
      reopening or destroying application work.
- [ ] A repeatable native daily-work smoke test covers the supported workflows,
      repeated captures and input, and complete cleanup. Full format, lint, test,
      and build checks pass.

## Implementation order

1. Fix focus/navigation and truthful agent actions; bound settings transport and
   restore access to host audio services from the private runtime.
2. Add standard window states, reachable dialogs, and a running-window selector.
   Extend semantic agent access and Unicode clipboard operations.
3. Complete session and desktop service integration, persistence, and operational
   documentation. Use existing Smithay and desktop protocol implementations.
4. Exercise everyday apps on the actual display; fix observed failures and retain
   screenshots, metadata, logs, and recovery evidence. Reconcile older plans with
   tested capabilities.

## Baseline findings

The native harness, three-finger zoom, clipboard bridge, and private AT-SPI
access already exist. The audit found no compositor keyboard shortcuts, no
window switching/recovery, missing standard window states, incorrect activation
state, false-success launch responses, unbounded settings I/O, and private
runtime audio sockets missing from session configuration. These are concrete
daily-use blockers rather than a reason to implement every optional protocol.

An intermittent native capture timeout remains unresolved in
`/tmp/hearthspace-three-finger-native-1`; isolated retries passed unchanged.
Bounded settings I/O removes one independently identified class of stalls, but
does not establish the cause of that earlier failure.

## Evidence and limits

Record incremental commits, checks, native artifacts, and unresolved issues here
as work progresses. Hardware claims apply to the actual tested GPU and outputs;
unavailable hardware matrix cases remain in the native backend plan.
