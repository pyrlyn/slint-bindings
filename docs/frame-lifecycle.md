# How a frame happens

How a host view drives Slint through the `sb_*` C ABI, from resize to presentation.

## How a frame happens

1. The host view tracks its size and backing scale and calls `sb_host_resize`.
2. On every display-link tick (`CADisplayLink` on macOS,
   `CompositionTarget.Rendering` on WinUI) the host calls `sb_tick`, then
   either `sb_host_gpu_render` or `sb_host_render`. Slint repaints only when
   something changed.
3. GPU presentation is a `CAMetalLayer` sublayer on macOS
   (`sb_demo_new_metal`) or a WinUI `SwapChainPanel` (`sb_demo_new_swapchain`).
   wgpu creates the swap chain. During a live resize the Metal layer sets
   `presentsWithTransaction` so the frame is not stretched. If that constructor
   fails, the host presents the CPU frame (`CGImage` layer contents take RGBA8;
   WinUI `WriteableBitmap` takes BGRA8 from `sb_host_render_bgra`).
4. Mouse, scroll, keyboard and focus events go back through `sb_host_*` in
   logical points from the top-left corner.

All calls happen on the host's UI thread: Slint objects are single-threaded.
