//! Slint images uploaded as premultiplied RGBA8 textures.

use std::collections::HashMap;
use std::rc::Rc;

use i_slint_core::graphics::{ImageCacheKey, SharedImageBuffer};

/// Frames an image may go unused before its texture is dropped.
const KEEP_FRAMES: u64 = 120;

pub(crate) struct GpuImage {
    /// Texture size in pixels.
    pub size: [u32; 2],
    pub smooth: wgpu::BindGroup,
    pub nearest: wgpu::BindGroup,
}

impl std::fmt::Debug for GpuImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuImage")
            .field("size", &self.size)
            .finish()
    }
}

/// Everything needed to turn pixels into a bindable texture.
pub(crate) struct Uploader<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub layout: &'a wgpu::BindGroupLayout,
    pub smooth: &'a wgpu::Sampler,
    pub nearest: &'a wgpu::Sampler,
}

impl Uploader<'_> {
    /// `pixels` is premultiplied RGBA8, tightly packed.
    pub fn upload(&self, width: u32, height: u32, pixels: &[u8]) -> Rc<GpuImage> {
        let size = wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        if width > 0 && height > 0 {
            self.queue.write_texture(
                texture.as_image_copy(),
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                size,
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = |sampler: &wgpu::Sampler| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("slintx-image"),
                layout: self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        };
        Rc::new(GpuImage {
            size: [width, height],
            smooth: bind(self.smooth),
            nearest: bind(self.nearest),
        })
    }
}

#[derive(Default)]
pub(crate) struct ImageCache {
    frame: u64,
    entries: HashMap<(ImageCacheKey, [u32; 2]), (Rc<GpuImage>, u64)>,
}

impl ImageCache {
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.entries
            .retain(|_, (_, used)| frame.saturating_sub(*used) <= KEEP_FRAMES);
    }

    /// Returns the cached texture for `key` at `size`, or uploads `buffer`.
    /// An image without a cache key is uploaded every time.
    pub fn get(
        &mut self,
        uploader: &Uploader<'_>,
        key: ImageCacheKey,
        buffer: impl FnOnce() -> Option<SharedImageBuffer>,
        size: [u32; 2],
    ) -> Option<Rc<GpuImage>> {
        let cacheable = !matches!(key, ImageCacheKey::Invalid);
        let slot = (key, size);
        if cacheable && let Some((image, used)) = self.entries.get_mut(&slot) {
            *used = self.frame;
            return Some(image.clone());
        }
        let buffer = buffer()?;
        let (width, height, pixels) = premultiplied_rgba(&buffer);
        let image = uploader.upload(width, height, &pixels);
        if cacheable {
            self.entries.insert(slot, (image.clone(), self.frame));
        }
        Some(image)
    }
}

pub(crate) fn premultiplied_rgba(buffer: &SharedImageBuffer) -> (u32, u32, Vec<u8>) {
    match buffer {
        SharedImageBuffer::RGB8(pixels) => (
            pixels.width(),
            pixels.height(),
            pixels
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, 255])
                .collect(),
        ),
        SharedImageBuffer::RGBA8(pixels) => (
            pixels.width(),
            pixels.height(),
            pixels
                .as_slice()
                .iter()
                .flat_map(|p| {
                    let a = u16::from(p.a);
                    let m = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
                    [m(p.r), m(p.g), m(p.b), p.a]
                })
                .collect(),
        ),
        SharedImageBuffer::RGBA8Premultiplied(pixels) => (
            pixels.width(),
            pixels.height(),
            pixels
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i_slint_core::graphics::{Rgba8Pixel, SharedPixelBuffer};

    #[test]
    fn straight_alpha_is_premultiplied() {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        buffer.make_mut_slice()[0] = Rgba8Pixel {
            r: 255,
            g: 128,
            b: 0,
            a: 128,
        };
        let (_, _, pixels) = premultiplied_rgba(&SharedImageBuffer::RGBA8(buffer));
        assert_eq!(pixels, [128, 64, 0, 128]);
    }
}
