//! Host-driven Slint embedding.
//!
//! A native host (an `NSView`, a WinUI panel) owns the run loop, the surface
//! and the input. This crate installs a Slint platform with no event loop of
//! its own: the host resizes, forwards input, ticks timers and asks for a
//! frame whenever it wants one. The CPU path paints into a buffer the host
//! owns. On macOS and Windows, FemtoVG can paint into a host surface instead.

pub mod a11y;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod gpu;
mod host;
mod ime;
mod keys;
mod platform;
pub mod runtime;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use gpu::GpuHost;
pub use host::{EmbeddedHost, Frame, PointerButton};
pub use keys::{
    AppKitKey, MOD_COMMAND, MOD_CONTROL, MOD_OPTION, MOD_SHIFT, VirtualKeyEvent, appkit_key_text,
    virtual_key_command, virtual_key_text,
};

/// Refuses a call that is not on the thread that first used Slint.
pub fn check_ui_thread() -> Result<(), Error> {
    slint_embed::thread::check().map_err(|_| Error::WrongThread)
}

/// Everything that can go wrong while embedding.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Another Slint platform (for example winit) was installed first.
    #[error("a different Slint platform is already installed in this process")]
    ForeignPlatform,
    /// This platform is already set, but for the other buffer type.
    #[error("the embedding platform is already installed with a different buffer type")]
    BufferMismatch,
    /// A call came from a thread other than the one that first used Slint.
    #[error(
        "called from a thread other than the one that first used Slint; \
         Slint is single-threaded"
    )]
    WrongThread,
    /// Slint itself refused an operation.
    #[error("slint: {0}")]
    Platform(#[from] slint::PlatformError),
    /// The component did not ask for a window during construction.
    #[error("the component was created without a window adapter")]
    NoWindow,
    /// The caller's buffer cannot hold a frame of the current size.
    #[error("buffer holds {got} bytes, a {width}x{height} RGBA frame needs {needed}")]
    BufferTooSmall {
        /// Bytes the caller passed.
        got: usize,
        /// Bytes the frame needs.
        needed: usize,
        /// Frame width in physical pixels.
        width: u32,
        /// Frame height in physical pixels.
        height: u32,
    },
    /// `scale` was zero, negative, or not finite. Logical size is pixels divided by scale.
    #[error("scale must be finite and greater than zero (got {scale})")]
    BadScale {
        /// The rejected scale, in physical pixels per logical point.
        scale: f32,
    },
    /// The host passed a null pointer or another value this API rejects.
    #[error("{0}")]
    InvalidArgument(&'static str),
    /// The GPU device or the host surface could not be created.
    ///
    /// Callers fall back to the CPU renderer. The text is the wgpu or Slint
    /// error, which is not a fixed set.
    #[error("gpu: {0}")]
    Gpu(String),
    /// A `.slint` file could not be read.
    #[error("cannot read {}: {source}", path.display())]
    Read {
        /// The file the caller named.
        path: std::path::PathBuf,
        /// What the file system reported.
        source: std::io::Error,
    },
    /// The `.slint` source did not compile; the text is the compiler's diagnostics.
    #[error("{0}")]
    Compile(String),
    /// The JSON bridge refused a property, callback or value (unknown name,
    /// wrong type, malformed JSON). The interpreter reports these as text.
    #[error("{0}")]
    Component(String),
}
