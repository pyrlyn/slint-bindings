//! The renderer behind Slint's renderer traits, for hosts that run Slint
//! itself. Slint lays text out (`sharedparley`) and walks the item tree; the
//! items are drawn through [`Canvas`](crate::Canvas).

mod frame;

use std::cell::OnceCell;
use std::pin::Pin;
use std::rc::{Rc, Weak};

use i_slint_core::api::PhysicalSize;
use i_slint_core::graphics::{Rgba8Pixel, SharedPixelBuffer};
use i_slint_core::item_rendering::render_component_items;
use i_slint_core::item_tree::{ItemTreeRef, ItemTreeWeak};
use i_slint_core::items::ItemRef;
use i_slint_core::platform::PlatformError;
use i_slint_core::renderer::RendererSealed;
use i_slint_core::textlayout::sharedparley::TextLayoutCache;
use i_slint_core::window::{WindowAdapter, WindowInner};

use crate::Renderer;
use frame::Frame;

/// Draws one Slint window with wgpu. Create one per window adapter.
pub struct WgpuRenderer {
    renderer: Renderer,
    window_adapter: OnceCell<Weak<dyn WindowAdapter>>,
    text_layout_cache: TextLayoutCache,
}

impl WgpuRenderer {
    /// The device and queue may be shared with the host and other renderers.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self::from_renderer(Renderer::new(device, queue))
    }

    /// Opens a device on the default adapter, for hosts without their own
    /// (tests, offscreen rendering).
    pub fn headless() -> Result<Self, PlatformError> {
        Renderer::headless()
            .map(Self::from_renderer)
            .map_err(|err| PlatformError::Other(err.to_string()))
    }

    fn from_renderer(renderer: Renderer) -> Self {
        Self {
            renderer,
            window_adapter: OnceCell::new(),
            text_layout_cache: TextLayoutCache::default(),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        self.renderer.device()
    }

    pub fn queue(&self) -> &wgpu::Queue {
        self.renderer.queue()
    }

    /// Draws the window's items into `target`; see [`Renderer::render`] for
    /// the formats it takes.
    pub fn render_to_texture(&self, target: &wgpu::Texture) -> Result<(), PlatformError> {
        let adapter = self.adapter()?;
        self.renderer
            .render(target, |canvas| self.draw_window(&adapter, canvas));
        Ok(())
    }

    /// Draws into a new `Rgba8Unorm` texture of the window's size and reads it
    /// back as straight-alpha RGBA8.
    pub fn render_to_pixels(&self) -> Result<SharedPixelBuffer<Rgba8Pixel>, PlatformError> {
        let adapter = self.adapter()?;
        let size = adapter.size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        let bytes = self
            .renderer
            .render_to_pixels([width, height], |canvas| self.draw_window(&adapter, canvas))
            .map_err(|err| PlatformError::Other(err.to_string()))?;
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
        let (pixels, _) = bytes.as_chunks::<4>();
        for (out, &[r, g, b, a]) in buffer.make_mut_slice().iter_mut().zip(pixels) {
            *out = Rgba8Pixel { r, g, b, a };
        }
        Ok(buffer)
    }

    fn adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.window_adapter()
            .ok_or_else(|| PlatformError::Other("the renderer has no window".into()))
    }

    fn draw_window(&self, adapter: &Rc<dyn WindowAdapter>, canvas: &mut crate::Canvas<'_>) {
        let window = WindowInner::from_pub(adapter.window());
        let scale = adapter.window().scale_factor();
        window.draw_contents(|components, post_render| {
            let mut frame = Frame::new(canvas, window, &self.text_layout_cache, scale);
            for (component, origin) in components {
                if let Some(component) = ItemTreeWeak::upgrade(component) {
                    render_component_items(&component, &mut frame, *origin, adapter);
                }
            }
            post_render(&mut frame);
        });
    }
}

impl RendererSealed for WgpuRenderer {
    fn text_layout_cache(&self) -> Option<&TextLayoutCache> {
        Some(&self.text_layout_cache)
    }

    fn free_graphics_resources(
        &self,
        component: ItemTreeRef,
        _items: &mut dyn Iterator<Item = Pin<ItemRef<'_>>>,
    ) -> Result<(), PlatformError> {
        self.text_layout_cache.component_destroyed(component);
        Ok(())
    }

    fn set_window_adapter(&self, window_adapter: &Rc<dyn WindowAdapter>) {
        let _ = self.window_adapter.set(Rc::downgrade(window_adapter));
    }

    fn window_adapter(&self) -> Option<Rc<dyn WindowAdapter>> {
        self.window_adapter.get().and_then(Weak::upgrade)
    }

    fn resize(&self, _size: PhysicalSize) -> Result<(), PlatformError> {
        Ok(())
    }

    fn take_snapshot(&self) -> Result<SharedPixelBuffer<Rgba8Pixel>, PlatformError> {
        self.render_to_pixels()
    }

    fn supports_transformations(&self) -> bool {
        false
    }
}
