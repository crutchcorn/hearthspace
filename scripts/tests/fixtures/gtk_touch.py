"""GTK 3 CSD acceptance client using the toolkit's ordinary HeaderBar gesture."""

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

window = Gtk.Window(title="Hearthspace GTK3 touch fixture")
window.set_default_size(520, 380)
header = Gtk.HeaderBar(title="GTK3 touch fixture")
header.get_accessible().set_name("GTK3 fixture titlebar")
header.set_show_close_button(True)
window.set_titlebar(header)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
box.set_border_width(24)
counter = Gtk.Label(label="Touch count: 0")
button = Gtk.Button(label="Count touch")
count = 0


def clicked(_button):
    global count
    count += 1
    counter.set_text(f"Touch count: {count}")


button.connect("clicked", clicked)
box.pack_start(counter, False, False, 0)
box.pack_start(button, False, False, 0)
marker = Gtk.Grid(column_spacing=6, row_spacing=6)
marker.set_column_homogeneous(True)
marker.set_row_homogeneous(True)
style = Gtk.CssProvider()
style.load_from_data(
    b".touch-light { background-color: #27b340; color: #fff; }"
    b".touch-dark { background-color: #194d26; color: #fff; }"
)
for row in range(4):
    for column in range(8):
        cell = Gtk.Label(label=f"{chr(65 + column)}{row + 1}")
        cell.set_size_request(44, 32)
        context = cell.get_style_context()
        context.add_class("touch-light" if (row + column) % 2 else "touch-dark")
        context.add_provider(style, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        marker.attach(cell, column, row, 1, 1)
box.pack_start(marker, True, True, 0)
window.add(box)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()
