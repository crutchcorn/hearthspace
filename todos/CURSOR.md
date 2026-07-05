- [ ] Automatic hardware-to-software fallback is not wired yet.
- [ ] Software cursor shape changes are not wired yet.
- [ ] Client-provided Wayland cursor images are ignored.
- [ ] Native cursor handling uses legacy DRM cursor calls plus a one-time atomic
  cursor-plane clear. Future atomic cursor-plane ownership should preserve the
  same high-level policy: hardware cursor first, software cursor fallback, and
  Xcursor assets as the art source.
