//! A 2D renderer written on wgpu, for slintx and for Slint itself.
//!
//! It replaces Slint's GPU renderers (FemtoVG, Skia): none of their code is in
//! this crate or its dependency graph. Glyphs are rasterized with swash, paths
//! tessellated with lyon and everything else drawn as signed distance fields
//! in one WGSL pipeline.
//!
//! [`Renderer::render`] hands a [`Canvas`] to the caller, which draws with the
//! crate's own [`Brush`], [`Image`] and path types. [`WgpuRenderer`] does the
//! same for a Slint window: Slint walks the item tree and lays text out
//! (`sharedparley`), and the items are drawn through a [`Canvas`].
//!
//! Not supported yet: rotation and scaling (like Slint's software renderer,
//! [`WgpuRenderer`] reports no support for transformations), layers (children
//! of an `Opacity` fade one by one), inset box shadows, and more than one
//! rounded clip at a time.

mod adapter;
mod canvas;
mod glyphs;
mod gpu;
mod images;
mod renderer;
mod scene;
mod types;

pub use lyon_tessellation::path;
pub use parley;
pub use wgpu;

pub use crate::adapter::WgpuRenderer;
pub use crate::canvas::{Canvas, GlyphRun, ImageStyle, Paint};
pub use crate::renderer::{Error, Renderer};
pub use crate::types::{
    Brush, Color, FillRule, GradientStop, Image, ImageFit, LineCap, LineJoin, PathPlacement,
    Pixels, Radii, Rect, Stroke, Tiling,
};
