#!/usr/bin/env python3
"""Shows a GtkBuilder file's window at a given content size, prints every
widget named by `--source-tags` as `key x y width height` in the window's
coordinates, and saves the window as a PNG.

Usage: gtk_geometry.py <file.ui> <out.png> <width> <height>
"""

import sys
import time

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Graphene", "1.0")
from gi.repository import GLib, Graphene, Gtk  # noqa: E402


def settle(window, width, height):
    """Runs the main loop until the window has its size, then a few frames
    more so that late size requests are laid out too. A window whose content
    needs more keeps its larger size and is measured as it is."""
    context = GLib.MainContext.default()
    deadline = time.monotonic() + 5
    while (window.get_width(), window.get_height()) != (width, height):
        if time.monotonic() > deadline:
            print(f"window is {window.get_width()}x{window.get_height()}", file=sys.stderr)
            break
        context.iteration(False)
    end = time.monotonic() + 0.5
    while time.monotonic() < end:
        context.iteration(False)


def walk(widget, window, out):
    if not widget.get_visible():
        return
    name = widget.get_name()
    # Untagged widgets keep GTK's default name, the class name. The window
    # is the frame itself, which Slint does not list.
    if ":" in name and widget is not window:
        ok, bounds = widget.compute_bounds(window)
        if ok and bounds.get_width() > 0 and bounds.get_height() > 0:
            x, y, w, h = bounds.get_x(), bounds.get_y(), bounds.get_width(), bounds.get_height()
            # A layout's padding is its margin here, outside its bounds; a
            # Slint layout's rectangle takes the padding in. In an overlay
            # the margins are the position instead.
            if isinstance(widget, (Gtk.Box, Gtk.Grid)) and not isinstance(
                widget.get_parent(), Gtk.Overlay
            ):
                start, end = widget.get_margin_start(), widget.get_margin_end()
                top, bottom = widget.get_margin_top(), widget.get_margin_bottom()
                x, y, w, h = x - start, y - top, w + start + end, h + top + bottom
            out.append(f"{name} {x:g} {y:g} {w:g} {h:g}")
    child = widget.get_first_child()
    while child is not None:
        walk(child, window, out)
        child = child.get_next_sibling()


class Handlers:
    """Every signal handler the file names, as a no-op: the converter keeps
    handlers as names for the page code, which is not here."""

    def __getattr__(self, name):
        return lambda *args: None


def main():
    ui, png, width, height = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    Gtk.init()
    builder = Gtk.Builder(Handlers())
    builder.add_from_file(ui)
    window = next(o for o in builder.get_objects() if isinstance(o, Gtk.Window))
    # No title bar or shadow: the window is exactly the content.
    window.set_decorated(False)
    window.set_default_size(width, height)
    window.set_resizable(False)
    window.present()
    settle(window, width, height)

    out = []
    walk(window, window, out)
    print("\n".join(out))

    width, height = window.get_width(), window.get_height()
    snapshot = Gtk.Snapshot()
    Gtk.WidgetPaintable.new(window).snapshot(snapshot, width, height)
    node = snapshot.to_node()
    bounds = Graphene.Rect().init(0, 0, width, height)
    texture = window.get_renderer().render_texture(node, bounds)
    texture.save_to_png(png)


main()
