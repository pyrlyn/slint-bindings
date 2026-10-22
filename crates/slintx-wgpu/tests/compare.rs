//! Screenshot comparison: each scene is drawn by Slint's software renderer
//! (the reference) and by slintx-wgpu, then compared pixel by pixel.
//!
//! Antialiasing and glyph rasterizing differ between the two, so a pixel may
//! differ by up to `CHANNEL_THRESHOLD` per channel, and a scene may have a
//! small share of pixels past that. The three frames of each scene (software,
//! wgpu, diff) are written under `target/tmp/slintx-wgpu/`.
//!
//! The software renderer samples images nearest-neighbour whatever their
//! `image-rendering`, so the compared images are pixelated. It draws no box shadows and clips to rectangles only
//! (its `draw_box_shadow` and the radius in `combine_clip` are TODOs in
//! 1.18), so those two are checked against expected pixels instead.
//!
//! Without a GPU adapter the tests pass without comparing, like the core's
//! GPU smoke test.

use std::cell::Cell;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use slint::platform::software_renderer::RepaintBufferType;
use slint::platform::{Renderer, WindowAdapter, WindowEvent};
use slint::{
    ComponentHandle, Image, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, Window, WindowSize,
};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
use slintx_wgpu::WgpuRenderer;

type TestResult = Result<(), Box<dyn Error>>;

/// Largest per-channel difference that still counts as the same pixel.
const CHANNEL_THRESHOLD: u8 = 48;

/// A test window that draws with slintx-wgpu.
struct GpuWindow {
    window: Window,
    renderer: WgpuRenderer,
    size: Cell<PhysicalSize>,
}

impl WindowAdapter for GpuWindow {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn set_size(&self, size: WindowSize) {
        let scale = self.window.scale_factor();
        self.size.set(size.to_physical(scale));
        self.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(scale),
        });
    }
}

fn compile(source: &str, path: &Path) -> Result<ComponentDefinition, Box<dyn Error>> {
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(source.into(), path.into()),
    );
    let errors: Vec<String> = result
        .diagnostics()
        .filter(|d| d.level() == slint_interpreter::DiagnosticLevel::Error)
        .map(|d| d.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("\n").into());
    }
    let name = result
        .component_names()
        .last()
        .ok_or("no exported component")?
        .to_owned();
    Ok(result.component(&name).ok_or("component vanished")?)
}

/// Sizes the window and returns its frame as straight-alpha RGBA8 bytes.
fn snapshot(
    instance: &ComponentInstance,
    size: [u32; 2],
    scale: f32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let window = instance.window();
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    window.set_size(PhysicalSize::new(size[0], size[1]));
    instance.show()?;
    let frame = window.take_snapshot()?;
    instance.hide()?;
    Ok(frame.as_bytes().to_vec())
}

type Setup<'a> = &'a dyn Fn(&ComponentInstance) -> TestResult;

/// Builds `definition` once per renderer, applies `setup`, and checks that
/// at most `budget` (a share of all pixels) differ past the threshold.
fn compare(
    name: &str,
    definition: &ComponentDefinition,
    setup: Setup,
    size: [u32; 2],
    scale: f32,
    budget: f64,
) -> TestResult {
    let Some(frame) = gpu_frame(name, definition, setup, size, scale)? else {
        return Ok(());
    };
    let software = definition.create()?;
    setup(&software)?;
    let reference = snapshot(&software, size, scale)?;

    assert_eq!(reference.len(), frame.len(), "{name}: frame sizes differ");
    let differing = rgba_png::differing_pixels(&reference, &frame, CHANNEL_THRESHOLD);
    let total = (size[0] * size[1]) as usize;
    let share = differing as f64 / total as f64;
    eprintln!(
        "{name}: {:.2}% of pixels differ, {} not identical",
        share * 100.0,
        rgba_png::differing_pixels(&reference, &frame, 0),
    );
    // Always written, so a passing run can be looked at too.
    let dir = write_frames(name, size, &reference, &frame)?;
    assert!(
        share <= budget,
        "{name}: {differing} of {total} pixels differ ({:.2}% > {:.2}%), frames in {}",
        share * 100.0,
        budget * 100.0,
        dir.display(),
    );
    Ok(())
}

/// The slintx-wgpu frame, or `None` when this machine has no GPU adapter.
fn gpu_frame(
    name: &str,
    definition: &ComponentDefinition,
    setup: Setup,
    size: [u32; 2],
    scale: f32,
) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    slint_embed::platform::ensure_installed(RepaintBufferType::NewBuffer)?;
    let renderer = match WgpuRenderer::headless() {
        Ok(renderer) => renderer,
        Err(err) => {
            eprintln!("skipping {name}: {err}");
            return Ok(None);
        }
    };
    let adapter: Rc<dyn WindowAdapter> = Rc::new_cyclic(|weak: &Weak<GpuWindow>| GpuWindow {
        window: Window::new(weak.clone()),
        renderer,
        size: Cell::new(PhysicalSize::new(size[0], size[1])),
    });
    slint_embed::platform::set_next_window(Box::new(move || Ok(adapter)));
    let gpu = definition.create();
    // A staged window that creation did not consume would go to the next
    // software window.
    slint_embed::platform::clear_next_window();
    let gpu = gpu?;
    setup(&gpu)?;
    Ok(Some(snapshot(&gpu, size, scale)?))
}

fn write_frames(
    name: &str,
    size: [u32; 2],
    reference: &[u8],
    frame: &[u8],
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("slintx-wgpu");
    std::fs::create_dir_all(&dir)?;
    let diff = rgba_png::diff_image(reference, frame, CHANNEL_THRESHOLD);
    for (suffix, pixels) in [("software", reference), ("wgpu", frame), ("diff", &diff)] {
        let png = rgba_png::encode_png(size[0], size[1], pixels)?;
        std::fs::write(dir.join(format!("{name}-{suffix}.png")), png)?;
    }
    Ok(dir)
}

fn no_setup(_: &ComponentInstance) -> TestResult {
    Ok(())
}

/// Compiles an inline scene and compares it at 1× and 2×.
fn scene(name: &str, source: &str, budget: f64) -> TestResult {
    let definition = compile(source, Path::new(&format!("{name}.slint")))?;
    for scale in [1.0, 2.0] {
        let size = [(160.0 * scale) as u32, (120.0 * scale) as u32];
        compare(
            &format!("{name}@{scale}x"),
            &definition,
            &no_setup,
            size,
            scale,
            budget,
        )?;
    }
    Ok(())
}

#[test]
fn rectangles_and_borders() -> TestResult {
    scene(
        "rectangles",
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: white;
            Rectangle { x: 8px; y: 8px; width: 60px; height: 40px; background: #3366cc; }
            Rectangle { x: 80px; y: 8px; width: 70px; height: 40px; background: #e04040;
                border-radius: 12px; }
            Rectangle { x: 8px; y: 60px; width: 60px; height: 50px; background: #fff0c0;
                border-width: 3px; border-color: #206020; }
            Rectangle { x: 80px; y: 60px; width: 70px; height: 50px; background: #80ffffff;
                border-width: 4px; border-color: #8030a0;
                border-top-left-radius: 20px; border-bottom-right-radius: 6px; }
        }
        "#,
        0.002,
    )
}

#[test]
fn gradients() -> TestResult {
    scene(
        "gradients",
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: white;
            Rectangle { x: 4px; y: 4px; width: 72px; height: 52px;
                background: @linear-gradient(45deg, #ff0000 0%, #0000ff 100%); }
            Rectangle { x: 84px; y: 4px; width: 72px; height: 52px; border-radius: 10px;
                background: @linear-gradient(90deg, #00000000 0%, #208020 50%, #ffff00 100%); }
            Rectangle { x: 4px; y: 64px; width: 72px; height: 52px;
                background: @radial-gradient(circle, #ffffff 0%, #ff8000 60%, #400000 100%); }
            Rectangle { x: 84px; y: 64px; width: 72px; height: 52px;
                background: @conic-gradient(#ff0000 0deg, #00ff00 120deg, #0000ff 240deg, #ff0000 360deg); }
        }
        "#,
        0.006,
    )
}

#[test]
fn clips_and_opacity() -> TestResult {
    scene(
        "clips",
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: #eeeeee;
            Rectangle { x: 10px; y: 10px; width: 60px; height: 60px; clip: true;
                Rectangle { x: -20px; y: 20px; width: 100px; height: 20px; background: #2060c0; }
            }
            Rectangle { x: 85px; y: 10px; width: 60px; height: 60px; clip: true;
                Rectangle { x: 10px; y: -10px; width: 20px; height: 100px; background: #c02060; }
                Rectangle { x: 30px; y: 30px; width: 60px; height: 60px; clip: true;
                    Rectangle { width: 100px; height: 10px; background: #20c060; }
                }
            }
            Rectangle { x: 10px; y: 80px; width: 140px; height: 30px; opacity: 0.5;
                Rectangle { width: 70px; height: 30px; background: black; }
                Rectangle { x: 70px; width: 70px; height: 30px; background: #ff0000; }
            }
        }
        "#,
        0.002,
    )
}

#[test]
fn text() -> TestResult {
    scene(
        "text",
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: white;
            VerticalLayout {
                padding: 6px;
                Text { text: "Hello, wgpu!"; font-size: 18px; color: #202020; }
                Text { text: "Small grey text 0123"; font-size: 11px; color: #707070; }
                Text { text: "Bold red"; font-size: 16px; font-weight: 700; color: #c02020; }
                Text { text: "Italic"; font-size: 16px; font-italic: true; color: #2040a0; }
            }
        }
        "#,
        0.002,
    )
}

#[test]
fn paths() -> TestResult {
    scene(
        "paths",
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: white;
            Path { x: 10px; y: 10px; width: 60px; height: 60px;
                viewbox-width: 60; viewbox-height: 60;
                commands: "M 30 0 L 60 60 L 0 60 Z";
                fill: #30a050; }
            Path { x: 90px; y: 10px; width: 60px; height: 60px;
                viewbox-width: 60; viewbox-height: 60;
                commands: "M 0 30 C 15 0 45 60 60 30";
                stroke: #2040c0; stroke-width: 4px; }
            Path { x: 40px; y: 80px; width: 80px; height: 30px;
                viewbox-width: 80; viewbox-height: 30;
                commands: "M 0 0 L 80 0 L 80 30 L 0 30 Z";
                fill: #ffe080; stroke: #806000; stroke-width: 2px; }
        }
        "#,
        0.008,
    )
}

#[test]
fn images() -> TestResult {
    let definition = compile(
        r#"
        export component Scene inherits Window {
            in property <image> picture;
            width: 160px; height: 120px;
            background: white;
            Image { x: 4px; y: 4px; width: 72px; height: 52px; source: picture;
                image-fit: fill; image-rendering: pixelated; }
            Image { x: 84px; y: 4px; width: 72px; height: 52px; source: picture;
                image-fit: contain; image-rendering: pixelated; }
            Image { x: 4px; y: 64px; width: 72px; height: 52px; source: picture;
                colorize: #c02080; image-rendering: pixelated; }
            Image { x: 84px; y: 64px; width: 72px; height: 52px; source: picture;
                image-fit: preserve; image-rendering: pixelated;
                horizontal-tiling: repeat; vertical-tiling: repeat; }
        }
        "#,
        Path::new("images.slint"),
    )?;
    let setup = |instance: &ComponentInstance| -> TestResult {
        instance.set_property("picture", Value::Image(checkerboard()))?;
        Ok(())
    };
    // The software renderer picks texels from the pixel's left edge in 8-bit
    // fixed point; slintx-wgpu samples at the pixel centre like any GPU, so
    // scaled cell edges land up to a pixel apart.
    compare("images@1x", &definition, &setup, [160, 120], 1.0, 0.05)?;
    compare("images@2x", &definition, &setup, [320, 240], 2.0, 0.05)
}

/// A 16×16 picture with hard edges and a translucent corner.
fn checkerboard() -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(16, 16);
    for (index, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (index % 16, index / 16);
        *pixel = if (x / 4 + y / 4) % 2 == 0 {
            Rgba8Pixel::new(20, 60, 200, 255)
        } else if x >= 12 && y >= 12 {
            Rgba8Pixel::new(255, 200, 0, 128)
        } else {
            Rgba8Pixel::new(240, 240, 240, 255)
        };
    }
    Image::from_rgba8(buffer)
}

/// The core's demo form: std-widgets, text, a line edit and a button.
#[test]
fn demo_form() -> TestResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slint-bindings-core/ui/demo.slint");
    let definition = compile(&std::fs::read_to_string(&path)?, &path)?;
    compare("demo@1x", &definition, &no_setup, [320, 200], 1.0, 0.002)?;
    compare("demo@2x", &definition, &no_setup, [640, 400], 2.0, 0.002)
}

/// What the software renderer cannot draw: a drop shadow and a rounded clip.
#[test]
fn shadows_and_rounded_clips() -> TestResult {
    let definition = compile(
        r#"
        export component Scene inherits Window {
            width: 160px; height: 120px;
            background: white;
            Rectangle { x: 20px; y: 20px; width: 50px; height: 50px; background: #4080ff;
                drop-shadow-blur: 8px; drop-shadow-color: black;
                drop-shadow-offset-x: 6px; drop-shadow-offset-y: 6px; }
            Rectangle { x: 90px; y: 20px; width: 60px; height: 60px; clip: true;
                border-radius: 30px;
                Rectangle { width: 100%; height: 100%; background: #c02060; }
            }
        }
        "#,
        Path::new("shadows.slint"),
    )?;
    let Some(frame) = gpu_frame("shadows", &definition, &no_setup, [160, 120], 1.0)? else {
        return Ok(());
    };
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let at = (y * 160 + x) * 4;
        [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
    };
    // Below the box, inside the offset shadow: darkened.
    assert!(
        pixel(45, 73)[0] < 160,
        "no shadow under the box: {:?}",
        pixel(45, 73)
    );
    // Far from the box: untouched background.
    assert_eq!(pixel(5, 110), [255, 255, 255, 255]);
    // The box itself covers its shadow.
    assert_eq!(pixel(45, 45), [0x40, 0x80, 0xff, 255]);
    // The rounded clip cuts the child's corner but keeps its centre.
    assert_eq!(pixel(92, 22), [255, 255, 255, 255]);
    assert_eq!(pixel(120, 50), [0xc0, 0x20, 0x60, 255]);
    Ok(())
}
