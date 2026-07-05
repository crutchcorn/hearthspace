- [x] Automatic hardware-to-software fallback is wired: the native backend
  presents the cursor in software when a device has no usable hardware cursor
  plane, when a DRM cursor op fails at runtime, or when a client is drawing its
  own cursor surface. Headless always uses the software cursor.
- [x] Software cursor shape changes are wired: the software cursor rebuilds its
  buffer whenever the desired named shape changes (default/resize cursors).
- [x] Client-provided Wayland cursor images are honored: a focused client's
  `wl_pointer.set_cursor` surface is presented as a software cursor at the
  client-supplied hotspot, and the compositor's named cursor takes back over
  when the pointer leaves the client's content.
- [ ] Native cursor handling uses legacy DRM cursor calls plus a one-time atomic
  cursor-plane clear. Future atomic cursor-plane ownership should preserve the
  same high-level policy: hardware cursor first, software cursor fallback, and
  Xcursor assets as the art source.
- [ ] Client cursor surfaces are always presented in software (composited into
  the scene). Uploading a client cursor buffer into the DRM cursor plane could
  keep the low-latency hardware path for client cursors too.
