# Ideas

Not approved yet.

- Upstream: offer Slint a minimal "embed into a foreign surface" example in Rust (discussion #6585 asks for it) once T3/T7 settle.
- Linux host (GTK4 `GtkGLArea` or a Wayland subsurface) on the same core.
- Use Slint's `ComponentContainer` to host several the UI description format screens in one Slint window instead of one native view each.
- Compare against Slint's own Swift bindings PR (#11167) if it lands upstream; drop our Swift layer in its favour where it overlaps.
