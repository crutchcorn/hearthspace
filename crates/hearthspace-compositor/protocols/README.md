# GTK protocol input

`gtk-shell.xml` is an unmodified copy of GTK's
[`gdk/wayland/protocol/gtk-shell.xml`](https://github.com/GNOME/gtk/blob/b3d08d3e60136da2522de3313cf4479e1d6fe20f/gdk/wayland/protocol/gtk-shell.xml)
at revision `b3d08d3e60136da2522de3313cf4479e1d6fe20f`.
SHA-256: `cc2717ba21182f637843e593f7aa2e543c1d6554a886053f400d81906967659d`.
GTK declares LGPL-2.1-or-later in that revision's
[`meson.build`](https://github.com/GNOME/gtk/blob/b3d08d3e60136da2522de3313cf4479e1d6fe20f/meson.build#L12).
The XML has no separate license notice; [GTK-COPYING](GTK-COPYING) preserves
upstream's license verbatim. These third-party inputs are not covered solely by
Hearthspace's Apache-2.0 license.

To refresh, copy both upstream files from one pinned revision, update the revision
and checksum above, and review changes before increasing the advertised version.
`wayland-scanner` generates Rust bindings directly from the XML. There are no local
XML modifications. No generated-code license exception is assumed.

Hearthspace advertises GTK shell v5; v6 additionally promises surface-offset
support. Valid explicit double-click requests use canvas maximization. Serial
authority comes from the latest delivered primary-button press or native touch
down, expires after one second, and is consumed once across all wrappers. The
originating surface and seat must match; touch also retains its device and
contact. Completed clicks and taps remain eligible briefly because GTK's request
can arrive after release. Completion requires the original implicit input grab
before release. Native touch requires the actual keyboard-focused surface without
moving pointer focus. Conflicting grabs and navigation/lock ownership still block
the operation. Accepted move/resize operations, cancellation, device removal and
session input reset invalidate outstanding authority.

GTK v5 also routes right/middle titlebar actions to the compositor without a
fallback response. Their current default visible behavior is preserved: GTK's
default right-click previously sent `xdg_toplevel.show_window_menu`, whose
Hearthspace handler has no menu, and middle-click defaults to no action. **Custom
GTK right/middle actions are not honored by this scoped implementation.** Adding
those actions requires explicit shell policy; ignoring a gesture does not ask GTK
to perform its fallback. See the upstream
[GTK3 dispatch](https://github.com/GNOME/gtk/blob/3.24.52/gtk/gtkwindow.c#L1383) and
[GTK4 dispatch](https://github.com/GNOME/gtk/blob/4.22.4/gtk/gtkwindowhandle.c#L336).

Capabilities are zero: menus and desktop icons remain client-owned. DBus/startup
metadata, bell, modality, `present`, and `request_focus` are advisory and do not
grant focus or override the existing xdg transient policy. GTK configure events
carry no extra tiled state; xdg carries maximized/fullscreen state. Resize-edge
events precede every toplevel configure, with pinned and fixed-size axes disabled,
so GTK versions that stop inferring edges when v2 is advertised retain resize
affordances. Committed size hints trigger a new sequence only when their effective
resize-edge mask changes. Resource handles are weak and released wrappers are
pruned.
