//! GPU smoke test. Its own process, so it does not share the software
//! platform installed by the crate's unit tests.
//!
//! A machine without a GPU adapter (or a cross-compile that cannot open one)
//! is a skip: the hosts fall back to the CPU renderer in that case.

#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::path::Path;

use slint_bindings_core::runtime::compile;
use slint_bindings_core::{Error, GpuHost};

#[test]
fn offscreen_frame_paints_when_a_device_exists() -> Result<(), Error> {
    let definition = compile(
        None,
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/demo.slint")),
    )?;
    let host = match GpuHost::new_offscreen(|| definition.create(), 64, 64, 1.0) {
        Ok(host) => host,
        Err(Error::Gpu(_)) => return Ok(()),
        Err(err) => return Err(err),
    };
    let frame = host.render_offscreen()?;
    assert!(frame.redrawn);
    Ok(())
}
