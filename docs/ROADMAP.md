# Roadmap

Hearthspace is being developed as a contender for everyday desktop use. The work
below focuses on gaps and broader acceptance coverage. It is not a release
schedule or a claim that every application, device, or recovery path has passed.
Current behavior belongs in the [maintained guides](README.md); recorded hardware
coverage belongs in [tested environments](TESTED_ENVIRONMENTS.md).

## Everyday acceptance

The native backend, graphical login, session lock, app launcher, clipboard,
power controls, and private automation interface are implemented. Broader testing
needs to establish how they behave together over sustained use:

- Repeat suspend and resume on AC and battery, preserving application identity,
  geometry, keyboard and pointing-device input, audio, and network connectivity.
  Include another active VT, wake with the lid still closed, docks, and external
  displays connected or removed around sleep. Measure longer suspend energy use
  separately from successful wake recovery. Follow [power acceptance](POWER_TESTING.md).
- Expand the output matrix: primary-output replacement, all outputs disconnected
  and reconnected, mixed scale and placement, and hotplug while locked or capturing.
  Check secondary-output capture and output removal during a pending capture.
  Fresh presentation and lock generations must remain tied to the correct output.
- Exercise multiple physical keyboards and pointing devices, device removal
  during held input, and touchscreen/trackpad behavior beyond virtual-input
  fixtures. Use [VT acceptance](VT_TESTING.md) and [GTK touch checks](GTK_TOUCH_TESTING.md).
- Extend [daily application workflows](DAILY_APPS_TESTING.md) across native
  Wayland, X11, and confined packages. Keep installed-session portal acceptance
  separate from the private native harness: confined file selection on that
  private bus remains a gap. Opening an app does not prove its file, clipboard,
  authentication, or media workflows.
- Measure responsiveness with many live windows, application redraw load,
  repeated input, and capture. Native GPU readback still runs on the compositor
  thread, although PNG encoding is off-thread. Attribute stalls to the actual
  phase before changing scheduling or capture behavior.

Record the tested revision, prerequisites, assertions, failures, and cleanup.
An input acknowledgment, screenshot, protocol fixture, and local password unlock
prove different things. Use [native testing](NATIVE_TESTING.md) for ownership and
[native recording](NATIVE_RECORDING.md) when motion is part of the assertion.

## Desktop functionality

- **Automatic idle policy:** add configurable lock, dimming, display-off, and
  optional suspend behavior with application inhibitors and AC/battery rules.
  The current [power coordinator](POWER.md) handles lid and suspend recovery;
  automatic idle and power-button policy remain with host configuration.
  Critical-battery actions and graphical power policy settings are separate
  work. Power profiles and hibernation need capability and recovery checks.
- **Application drag and drop:** complete general application-to-application
  transfers between Wayland and X11, including cancellation and lock boundaries.
  [Clipboard bridging](CLIPBOARD.md) and dragging Hearthspace's own
  [desktop files](DESKTOP_ITEMS.md) into Wayland or X11 apps already exist.
- **Network controls:** replace the Wi-Fi panel's instruction to use a network
  tray application with connection management and actionable status/errors.
- **Canvas drawing and handwriting:** freeform drawing objects and handwriting
  recognition for canvas search are not implemented. Keep these separate from
  the implemented [drawn app shortcuts](FAVORITE_APPS.md), lasso selection, and
  [text search](CANVAS_SEARCH.md).
- **Accessibility:** broaden keyboard and assistive-technology acceptance across
  shell controls and real applications. Improve the current title/app-ID
  heuristic used to associate compositor windows with AT-SPI objects. Missing
  application semantics must remain explicit; arbitrary text extraction is not
  an available fallback.

## Graphics and device work

These changes need their own acceptance criteria before becoming supported
capabilities. Keep Smithay responsible for the generic compositor mechanisms.

- **Cursor planes:** move native cursor updates from legacy DRM calls to atomic
  ownership, and investigate hardware upload of client cursor surfaces. Preserve
  Xcursor artwork, hotspots, scale, and the existing software fallback when a
  plane is unavailable or a cursor operation fails. See [cursors](CURSOR.md).
- **KMS damage hints:** renderer damage tracking is active, but KMS damage clips
  are disabled. Investigate per-device/output support detection, one retry of a
  rejected frame without clips after `EINVAL`, and disabling unsupported hints. Diagnostics
  must distinguish renderer damage from KMS submission. Retries must preserve
  capture and lock presentation identity without duplicate completions.
- **Direct scanout:** investigate Smithay's scanout helpers with composition
  fallback for unsupported buffers, transforms, cursors, capture, and locking.
  Rendering an application directly must preserve the same presentation and
  isolation contracts as composition.
- **Multiple GPUs:** the current backend renders outputs on one selected DRM
  device. Cross-device rendering needs explicit device selection, buffer and
  allocator ownership, import/fallback behavior, and hotplug recovery. Discovering
  additional DRM devices does not provide those capabilities.
- **Input devices:** add tablet-tool and general libinput switch handling, which
  the native event path currently ignores. Existing logind lid handling is a
  separate integration. Validate capabilities and removal/cancellation semantics
  with physical devices.
- **Later display work:** evaluate variable refresh, HDR/color management,
  runtime GPU selection, and output-global retirement against concrete needs.
  The current output removal path disables globals; any lifecycle change must
  preserve client and capture behavior. See [DRM/KMS](DRM.md).

## Design questions

Two provisional notes retain ideas that still need design decisions:

- [Window context](../notes/window-context.md): richer on-demand context and an
  optional MCP frontend. Existing [local-model window gathering](WINDOW_GROUPS.md)
  already compares window titles; richer content collection is separate work.
- [Window idle lifecycle](../notes/window-idle-lifecycle.md): whether per-window
  activity can inform resource policy without disrupting background work or
  losing unsaved data. The current tracker only logs transitions.

An optional local-model chat interface in the shell remains a product question.
The existing window-gathering feature makes relevance decisions; it isn't a chat
interface. Any proposal should explain the useful workflow and its relationship
to the separate window-context work before adding another permanent surface.

Minimization, workspaces, and additional task-management conventions remain
product questions alongside the existing canvas, window switcher, and frame
groups. Additional protocols should follow a concrete application need. These
ideas do not change the current [architecture and ownership](ARCHITECTURE.md).
