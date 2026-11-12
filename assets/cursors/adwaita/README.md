# Additional cursor shapes

Artwork by the [GNOME Project](https://www.gnome.org), from
[Adwaita Icon Theme 50.0](https://github.com/GNOME/adwaita-icon-theme/tree/50.0/Adwaita/cursors)
(tag tree `551245ae75fdc42cde42a8cf24ca2ccab9d3a815`). The upstream
[editable cursor sources](https://github.com/GNOME/adwaita-icon-theme/tree/50.0/src/cursors)
contain the SVG, PNGs, and generators. `COPYING` preserves upstream attribution
and licensing; this copy uses its LGPL v3 option, included in `LGPL-3.0` and
`GPL-3.0`.

Hearthspace retains the first animation frame at each nominal size, without
changing pixels or hotspots. Wait and progress indicators are static. The
`move` shape uses upstream `all-resize`; `dnd-ask` uses `context-menu`. Existing
canvas hand and resize assets in the parent directory remain unchanged.

To regenerate, unpack the upstream 50.0 source release and run from the
repository root:

```sh
python3 scripts/import_cursor_shapes.py /path/to/adwaita-icon-theme-50.0
```

The compositor embeds these files. There is no runtime icon-theme dependency.
