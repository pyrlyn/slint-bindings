//! Drives the demo form through the C ABI the hosts call. The form is
//! compiled at run time from `slint-bindings-core/ui/demo.slint`, the file
//! the hosts load.
//!
//! The rectangles in `ui/demo.slint` are the hit targets (logical px, origin
//! top-left): the name field at (160, 64) and Continue at (80, 112). Headless,
//! so `just check` does not need a window server.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;

use slint_bindings_ffi::{
    SbFrame, SbHost, SbPointerButton, run_on_test_ui_thread, sb_compile, sb_component_new,
    sb_definition_free, sb_get_property_json, sb_host_composition_commit,
    sb_host_composition_update, sb_host_focus_changed, sb_host_frame_len, sb_host_free,
    sb_host_is_gpu, sb_host_key_pressed, sb_host_key_released, sb_host_key_repeated,
    sb_host_pixel_size, sb_host_pointer_moved, sb_host_pointer_pressed, sb_host_pointer_released,
    sb_host_render, sb_host_resize, sb_invoke_json, sb_last_error, sb_on_callback,
    sb_set_property_json, sb_string_free, sb_tick,
};

const FORM_WIDTH: u32 = 320;
const FORM_HEIGHT: u32 = 200;
const FIELD_X: f32 = 160.0;
const FIELD_Y: f32 = 64.0;
const BUTTON_X: f32 = 80.0;
const BUTTON_Y: f32 = 112.0;
/// 800×600 pt at 2×, the M1 checkpoint size.
const RETINA_PIXEL_WIDTH: u32 = 1600;
const RETINA_PIXEL_HEIGHT: u32 = 1200;
const RETINA_SCALE: f32 = 2.0;

struct Host(*mut SbHost);

impl Drop for Host {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `sb_component_new` and is freed once.
        unsafe { sb_host_free(self.0) }
    }
}

fn last_error() -> String {
    let err = sb_last_error();
    if err.is_null() {
        return String::new();
    }
    // SAFETY: a non-null `sb_last_error` is NUL-terminated until the next call.
    unsafe { CStr::from_ptr(err) }
        .to_string_lossy()
        .into_owned()
}

fn demo_path() -> CString {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../slint-bindings-core/ui/demo.slint"
    );
    CString::new(path).unwrap_or_default()
}

fn open(width: u32, height: u32, scale: f32) -> Host {
    let path = demo_path();
    // SAFETY: `path` is NUL-terminated; a null source reads the file.
    let definition = unsafe { sb_compile(ptr::null(), path.as_ptr()) };
    assert!(!definition.is_null(), "sb_compile failed: {}", last_error());
    // SAFETY: `definition` is live until the free right after; the host does not borrow it.
    let host = unsafe { sb_component_new(definition, width, height, scale) };
    // SAFETY: from `sb_compile`, freed once.
    unsafe { sb_definition_free(definition) };
    assert!(!host.is_null(), "sb_component_new failed: {}", last_error());
    Host(host)
}

fn set_name(host: &Host, name: &str) {
    let property = CString::new("name").unwrap_or_default();
    let json = CString::new(format!("\"{name}\"")).unwrap_or_default();
    // SAFETY: `host` is live; both strings are NUL-terminated.
    let ok = unsafe { sb_set_property_json(host.0, property.as_ptr(), json.as_ptr()) };
    assert!(ok, "sb_set_property_json failed: {}", last_error());
}

fn render(host: &Host) -> (SbFrame, Vec<u8>) {
    // SAFETY: `host` is live until `Host` drops.
    unsafe {
        let mut buf = vec![0u8; sb_host_frame_len(host.0)];
        let frame = sb_host_render(host.0, buf.as_mut_ptr(), buf.len());
        assert_eq!(frame.failed, 0, "render failed");
        (frame, buf)
    }
}

fn click(host: &Host, x: f32, y: f32) {
    // SAFETY: `host` is live until `Host` drops.
    unsafe {
        assert!(sb_host_pointer_moved(host.0, x, y));
        assert!(sb_host_pointer_pressed(
            host.0,
            x,
            y,
            SbPointerButton::Left as u8
        ));
        assert!(sb_host_pointer_released(
            host.0,
            x,
            y,
            SbPointerButton::Left as u8
        ));
    }
}

fn type_ascii(host: &Host, text: &str) {
    for ch in text.chars() {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        let c = CString::new(&*encoded).unwrap_or_default();
        assert!(!c.to_bytes().is_empty());
        // SAFETY: `host` is live; `c` is NUL-terminated and outlives the calls.
        unsafe {
            assert!(sb_host_key_pressed(host.0, c.as_ptr()));
            assert!(sb_host_key_released(host.0, c.as_ptr()));
        }
    }
}

/// Collects `submitted(name)`: the arguments arrive as the JSON array `["name"]`.
extern "C" fn on_submitted(user_data: *mut c_void, args_json: *const c_char) {
    if user_data.is_null() || args_json.is_null() {
        return;
    }
    // SAFETY: the FFI passes a CString that is valid for this call.
    let args = unsafe { CStr::from_ptr(args_json) }.to_str().unwrap_or("");
    // Test names hold no quotes or escapes, so the array is `["` + name + `"]`.
    let name = args
        .strip_prefix("[\"")
        .and_then(|rest| rest.strip_suffix("\"]"))
        .unwrap_or(args);
    // SAFETY: the test passes a live `Vec<String>` it does not touch during the call.
    let slot = unsafe { &mut *user_data.cast::<Vec<String>>() };
    slot.push(name.to_owned());
}

fn listen(host: &Host, slot: *mut Vec<String>) {
    let name = CString::new("submitted").unwrap_or_default();
    // SAFETY: `host` is live; `slot` stays live for every callback this test fires.
    let ok = unsafe { sb_on_callback(host.0, name.as_ptr(), Some(on_submitted), slot.cast()) };
    assert!(ok, "sb_on_callback failed: {}", last_error());
}

#[test]
fn type_ascii_then_continue_submits_the_name() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let (frame, buf) = render(&host);
        assert_eq!(frame.redrawn, 1);
        assert!(buf.chunks(4).any(|px| px.len() == 4 && px[3] > 0));

        // SAFETY: `host` is live until the end of this test.
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);
        type_ascii(&host, "Ada");

        let mut submitted = Vec::new();
        listen(&host, &raw mut submitted);
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(submitted, ["Ada"]);

        let (frame, _) = render(&host);
        assert_eq!(frame.redrawn, 1);
    });
}

#[test]
fn japanese_preedit_commits_into_the_field() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        // SAFETY: `host` is live until the end of this test.
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);

        let preedit = CString::new("あ").unwrap_or_default();
        let committed = CString::new("日本語").unwrap_or_default();
        // SAFETY: `host` is live; both strings are NUL-terminated.
        unsafe {
            assert!(sb_host_composition_update(host.0, preedit.as_ptr(), 0, 1));
        }
        let mut submitted = Vec::new();
        listen(&host, &raw mut submitted);
        click(&host, BUTTON_X, BUTTON_Y);
        // The reading is still a preedit, so Continue submits an empty name.
        assert_eq!(submitted, [""]);

        click(&host, FIELD_X, FIELD_Y);
        // SAFETY: `host` is live; `committed` is NUL-terminated.
        unsafe {
            assert!(sb_host_composition_commit(host.0, committed.as_ptr()));
        }
        submitted.clear();
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(submitted, ["日本語"]);
    });
}

#[test]
fn tab_past_the_last_control_is_not_accepted() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        // SAFETY: `host` is live until the end of this test.
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);
        let tab = CString::new("\t").unwrap_or_default();
        // SAFETY: `host` is live; `tab` is NUL-terminated.
        unsafe {
            // Field → button: the view keeps the key.
            assert!(sb_host_key_pressed(host.0, tab.as_ptr()));
            assert!(sb_host_key_released(host.0, tab.as_ptr()));
            // Button → wrap: the host should move focus out, and that is not an error.
            assert!(!sb_host_key_pressed(host.0, tab.as_ptr()));
            assert!(sb_last_error().is_null());
        }
    });
}

#[test]
fn enter_in_the_field_submits() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);
        type_ascii(&host, "Bea");

        let mut submitted = Vec::new();
        listen(&host, &raw mut submitted);
        type_ascii(&host, "\n");
        assert_eq!(submitted, ["Bea"]);
    });
}

#[test]
fn host_set_name_reaches_the_submitted_callback() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        set_name(&host, "Ivan");
        let mut submitted = Vec::new();
        listen(&host, &raw mut submitted);
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(submitted, ["Ivan"]);
    });
}

#[test]
fn resize_changes_the_frame_and_a_repeat_does_not_repaint() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        // SAFETY: `host` is live.
        unsafe {
            assert!(sb_host_resize(host.0, FORM_WIDTH, FORM_HEIGHT, 1.0));
        }
        let (frame, _) = render(&host);
        assert_eq!(frame.redrawn, 0);

        unsafe {
            assert!(sb_host_resize(
                host.0,
                FORM_WIDTH * 2,
                FORM_HEIGHT * 2,
                RETINA_SCALE
            ));
            assert_eq!(
                sb_host_frame_len(host.0),
                (FORM_WIDTH * 2) as usize * (FORM_HEIGHT * 2) as usize * 4
            );
        }
        let (frame, buf) = render(&host);
        assert_eq!(frame.redrawn, 1);
        assert!(buf.chunks(4).any(|px| px.len() == 4 && px[3] > 0));
    });
}

#[test]
fn retina_demo_frame_paints() {
    run_on_test_ui_thread(|| {
        let host = open(RETINA_PIXEL_WIDTH, RETINA_PIXEL_HEIGHT, RETINA_SCALE);
        let started = std::time::Instant::now();
        let (frame, buf) = render(&host);
        let first = started.elapsed();
        assert_eq!(frame.redrawn, 1);
        assert!(buf.chunks(4).any(|px| px.len() == 4 && px[3] > 0));

        set_name(&host, "Ada");
        sb_tick();
        let started = std::time::Instant::now();
        let (frame, buf) = render(&host);
        let dirty = started.elapsed();
        assert_eq!(frame.failed, 0);
        assert_eq!(frame.redrawn, 1);

        if std::env::var_os("SB_FRAME_TIME").is_some() {
            eprintln!(
                "retina_first_frame_us={} retina_dirty_frame_us={}",
                first.as_micros(),
                dirty.as_micros()
            );
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/demo-frame.rgba");
            let mut bytes = Vec::with_capacity(buf.len() + 32);
            bytes.extend(format!("{RETINA_PIXEL_WIDTH} {RETINA_PIXEL_HEIGHT}\n").into_bytes());
            bytes.extend_from_slice(&buf);
            let _ = std::fs::write(path, bytes);
        }
    });
}

/// The caret blinks, so these frames are taken with nothing focused. A person
/// can open the PNG; the test compares pixels. `SB_UPDATE_SNAPSHOTS=1`
/// rewrites the files when that picture is meant to change.
fn snapshot_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/snapshots/{name}.png"))
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // The samples are already sRGB. The chunk stops a viewer from converting them.
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), png::DecodingError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.line_size * info.height as usize);
    Ok((info.width, info.height, buf))
}

fn must<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => {
            let message = err.to_string();
            assert!(message.is_empty(), "{message}");
            std::process::abort();
        }
    }
}

fn differing_pixels(left: &[u8], right: &[u8]) -> usize {
    left.chunks(4)
        .zip(right.chunks(4))
        .filter(|(a, b)| a != b)
        .count()
}

fn assert_snapshot(name: &str, width: u32, height: u32, rgba: &[u8]) {
    let path = snapshot_path(name);
    let encoded = must(encode_png(width, height, rgba));
    if std::env::var_os("SB_UPDATE_SNAPSHOTS").is_some() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &encoded);
        return;
    }
    assert!(
        path.is_file(),
        "missing snapshot {}; set SB_UPDATE_SNAPSHOTS=1 to write it",
        path.display()
    );
    let saved = std::fs::read(&path).unwrap_or_else(|err| {
        let message = err.to_string();
        assert!(message.is_empty(), "read {}: {message}", path.display());
        std::process::abort();
    });
    let (got_width, got_height, pixels) = must(decode_png(&saved));
    assert_eq!(
        (got_width, got_height, pixels.len()),
        (width, height, rgba.len()),
        "{name} snapshot size"
    );
    let diffs = differing_pixels(&pixels, rgba);
    if diffs == 0 {
        return;
    }
    let actual = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/snapshot-mismatches/{name}.png"));
    if let Some(parent) = actual.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&actual, encoded);
    assert_eq!(
        diffs,
        0,
        "{name}: {diffs} pixels differ from {}; wrote {}",
        path.display(),
        actual.display()
    );
}

fn paint(width: u32, height: u32, scale: f32, name: Option<&str>) -> Vec<u8> {
    let host = open(width, height, scale);
    let (frame, mut buf) = render(&host);
    assert_eq!(frame.redrawn, 1);
    if let Some(name) = name {
        set_name(&host, name);
        let (frame, named) = render(&host);
        assert_eq!(frame.redrawn, 1);
        buf = named;
    }
    // A blinking caret or a running animation would repaint immediately.
    let (again, _) = render(&host);
    assert_eq!(again.redrawn, 0, "frame is still changing");
    buf
}

#[test]
fn sign_in_frame_matches_the_snapshot() {
    run_on_test_ui_thread(|| {
        let first = paint(FORM_WIDTH, FORM_HEIGHT, 1.0, None);
        let second = paint(FORM_WIDTH, FORM_HEIGHT, 1.0, None);
        assert_eq!(
            differing_pixels(&first, &second),
            0,
            "the sign-in frame is not stable across two hosts"
        );
        // Goldens are rasterised on macOS. Other OSes use different fonts.
        if !cfg!(target_os = "macos") {
            return;
        }
        assert_snapshot("sign-in", FORM_WIDTH, FORM_HEIGHT, &first);

        let retina = paint(FORM_WIDTH * 2, FORM_HEIGHT * 2, RETINA_SCALE, None);
        assert_snapshot("sign-in@2x", FORM_WIDTH * 2, FORM_HEIGHT * 2, &retina);
    });
}

#[test]
fn named_field_frame_matches_the_snapshot() {
    run_on_test_ui_thread(|| {
        let buf = paint(FORM_WIDTH, FORM_HEIGHT, 1.0, Some("Ada"));
        if !cfg!(target_os = "macos") {
            return;
        }
        assert_snapshot("sign-in-ada", FORM_WIDTH, FORM_HEIGHT, &buf);
    });
}

#[test]
fn key_repeat_is_accepted() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let _ = render(&host);
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);
        let a = CString::new("a").unwrap_or_default();
        // SAFETY: `host` is live; `a` is NUL-terminated.
        unsafe {
            assert!(sb_host_key_pressed(host.0, a.as_ptr()));
            assert!(sb_host_key_repeated(host.0, a.as_ptr()));
            assert!(sb_host_key_released(host.0, a.as_ptr()));
        }
    });
}

/// Owned result of `sb_get_property_json` / `sb_invoke_json`; `None` for null.
fn take_string(text: *mut c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: a non-null result is a NUL-terminated string the caller owns.
    let owned = unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: from the FFI, freed once.
    unsafe { sb_string_free(text) };
    Some(owned)
}

fn get(host: &Host, name: &str) -> Option<String> {
    let name = CString::new(name).unwrap_or_default();
    // SAFETY: `host` is live; `name` is NUL-terminated.
    take_string(unsafe { sb_get_property_json(host.0, name.as_ptr()) })
}

fn set(host: &Host, name: &str, json: &str) -> bool {
    let name = CString::new(name).unwrap_or_default();
    let json = CString::new(json).unwrap_or_default();
    // SAFETY: `host` is live; both strings are NUL-terminated.
    unsafe { sb_set_property_json(host.0, name.as_ptr(), json.as_ptr()) }
}

#[test]
fn properties_round_trip_as_json_and_typing_shows_up() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        assert!(set(&host, "name", r#""Grace""#));
        assert_eq!(get(&host, "name").as_deref(), Some(r#""Grace""#));
        assert_eq!(get(&host, "submit-count").as_deref(), Some("0"));

        set_name(&host, "");
        let _ = render(&host);
        // SAFETY: `host` is live.
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, FIELD_X, FIELD_Y);
        type_ascii(&host, "Bo");
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(get(&host, "name").as_deref(), Some(r#""Bo""#));
        assert_eq!(get(&host, "submit-count").as_deref(), Some("1"));
    });
}

#[test]
fn bad_property_calls_fail_with_a_message() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        assert!(!set(&host, "no-such-property", "1"));
        assert!(
            last_error().contains("no-such-property"),
            "{}",
            last_error()
        );
        assert!(!set(&host, "name", "{not json"));
        assert!(!last_error().is_empty());
        assert!(!set(&host, "name", "42"), "a number is not a string");
        assert!(!last_error().is_empty());
        assert_eq!(get(&host, "no-such-property"), None);
        assert!(!last_error().is_empty());

        let name = CString::new("name").unwrap_or_default();
        let bad = [0x80u8, 0];
        // SAFETY: null and invalid UTF-8 are part of the contract; `host` is live.
        unsafe {
            assert!(!sb_set_property_json(host.0, ptr::null(), name.as_ptr()));
            assert!(!sb_set_property_json(host.0, name.as_ptr(), ptr::null()));
            assert!(!sb_set_property_json(
                host.0,
                name.as_ptr(),
                bad.as_ptr().cast()
            ));
            assert!(last_error().contains("UTF-8"), "{}", last_error());
            assert!(sb_get_property_json(ptr::null(), name.as_ptr()).is_null());
            sb_string_free(ptr::null_mut());
        }
    });
}

#[test]
fn invoking_a_callback_reaches_the_host_handler() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let mut submitted = Vec::new();
        listen(&host, &raw mut submitted);
        let name = CString::new("submitted").unwrap_or_default();
        let args = CString::new(r#"["Lin"]"#).unwrap_or_default();
        // SAFETY: `host` is live; the strings are NUL-terminated.
        let result = take_string(unsafe { sb_invoke_json(host.0, name.as_ptr(), args.as_ptr()) });
        assert_eq!(result.as_deref(), Some("null"));
        assert_eq!(submitted, ["Lin"]);

        let wrong = CString::new("[1, 2]").unwrap_or_default();
        // SAFETY: as above.
        let result = unsafe { sb_invoke_json(host.0, name.as_ptr(), wrong.as_ptr()) };
        assert!(result.is_null());
        assert!(last_error().contains("expects 1"), "{}", last_error());

        // SAFETY: as above; a null callback clears the handler.
        unsafe { assert!(sb_on_callback(host.0, name.as_ptr(), None, ptr::null_mut())) }
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(submitted, ["Lin"]);

        let unknown = CString::new("no-such-callback").unwrap_or_default();
        // SAFETY: as above.
        unsafe {
            assert!(!sb_on_callback(
                host.0,
                unknown.as_ptr(),
                Some(on_submitted),
                ptr::null_mut()
            ));
        }
        assert!(!last_error().is_empty());
    });
}

#[test]
fn compile_reports_diagnostics_and_missing_files() {
    run_on_test_ui_thread(|| {
        let source =
            CString::new("export component X inherits Window { foo bar }").unwrap_or_default();
        let path = CString::new("broken.slint").unwrap_or_default();
        // SAFETY: both strings are NUL-terminated.
        let definition = unsafe { sb_compile(source.as_ptr(), path.as_ptr()) };
        assert!(definition.is_null());
        assert!(last_error().contains("broken.slint"), "{}", last_error());

        let missing = CString::new("/nonexistent/form.slint").unwrap_or_default();
        // SAFETY: as above; a null source reads the file.
        let definition = unsafe { sb_compile(ptr::null(), missing.as_ptr()) };
        assert!(definition.is_null());
        assert!(
            last_error().contains("/nonexistent/form.slint"),
            "{}",
            last_error()
        );

        // SAFETY: null pointers are part of the contract.
        unsafe {
            assert!(sb_compile(source.as_ptr(), ptr::null()).is_null());
            assert!(sb_component_new(ptr::null(), 8, 8, 1.0).is_null());
            assert!(last_error().contains("definition"), "{}", last_error());
            sb_definition_free(ptr::null_mut());
        }
    });
}

#[test]
fn source_compiled_in_memory_renders_on_the_cpu() {
    run_on_test_ui_thread(|| {
        let source = CString::new(
            "export component Swatch inherits Window { background: #ff0000; in property <int> n; }",
        )
        .unwrap_or_default();
        let path = CString::new("swatch.slint").unwrap_or_default();
        // SAFETY: both strings are NUL-terminated.
        let definition = unsafe { sb_compile(source.as_ptr(), path.as_ptr()) };
        assert!(!definition.is_null(), "{}", last_error());
        // Two hosts from one definition, the second made after a frame of the first.
        // SAFETY: `definition` is live until the free below.
        let (first, second) = unsafe {
            let first = Host(sb_component_new(definition, 4, 4, 1.0));
            let second = Host(sb_component_new(definition, 4, 4, 1.0));
            sb_definition_free(definition);
            (first, second)
        };
        assert!(!first.0.is_null() && !second.0.is_null());
        // SAFETY: `first` is live.
        assert!(!unsafe { sb_host_is_gpu(first.0) });
        for host in [&first, &second] {
            let (frame, buf) = render(host);
            assert_eq!(frame.redrawn, 1);
            assert_eq!(&buf[..4], &[255, 0, 0, 255]);
        }
        assert!(set(&first, "n", "7"));
        assert_eq!(
            get(&second, "n").as_deref(),
            Some("0"),
            "hosts share no state"
        );
    });
}

#[test]
fn a_rust_crate_builds_the_host_and_hands_it_to_c() {
    run_on_test_ui_thread(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../slint-bindings-core/ui/demo.slint");
        let definition = match slint_bindings_core::runtime::compile(None, &path) {
            Ok(definition) => definition,
            Err(err) => panic_with(&err.to_string()),
        };
        let host = match SbHost::new(&definition, FORM_WIDTH, FORM_HEIGHT, 1.0) {
            Ok(host) => host,
            Err(err) => panic_with(&err.to_string()),
        };
        let seen = std::rc::Rc::new(std::cell::Cell::new(0));
        let sink = seen.clone();
        let set = host.component().set_callback("submitted", move |_| {
            sink.set(sink.get() + 1);
            slint_bindings_core::runtime::Value::Void
        });
        assert!(set.is_ok());
        let host = Host(host.into_raw());
        let _ = render(&host);
        click(&host, BUTTON_X, BUTTON_Y);
        assert_eq!(seen.get(), 1);
    });
}

fn panic_with(message: &str) -> ! {
    assert!(message.is_empty(), "{message}");
    std::process::abort();
}

#[cfg(target_os = "macos")]
#[test]
fn metal_without_a_layer_is_refused() {
    use slint_bindings_ffi::sb_component_new_metal;

    run_on_test_ui_thread(|| {
        let path = demo_path();
        // SAFETY: `path` is NUL-terminated; the definition is freed below.
        unsafe {
            let definition = sb_compile(ptr::null(), path.as_ptr());
            assert!(!definition.is_null());
            let host = sb_component_new_metal(definition, ptr::null_mut(), 8, 8, 1.0);
            assert!(host.is_null());
            assert!(last_error().contains("null"), "{}", last_error());
            sb_definition_free(definition);
        }
    });
}

// A screen weft's `weft-slint` generated (fixtures/weft/login.slint): Weft
// controls become std-widgets, and every action calls `perform(action, id)`.
// Hit targets in logical px at 480 wide: the email field's row at y 84, the
// submit button's at y 184, "Create an account" at (349, 220).
const WEFT_WIDTH: u32 = 480;
const WEFT_HEIGHT: u32 = 800;
const WEFT_EMAIL: (f32, f32) = (100.0, 84.0);
const WEFT_SUBMIT: (f32, f32) = (100.0, 184.0);
const WEFT_SIGNUP: (f32, f32) = (349.0, 220.0);

/// Collects each callback's raw JSON arguments.
extern "C" fn on_args(user_data: *mut c_void, args_json: *const c_char) {
    if user_data.is_null() || args_json.is_null() {
        return;
    }
    // SAFETY: the FFI passes a CString that is valid for this call.
    let args = unsafe { CStr::from_ptr(args_json) }.to_string_lossy();
    // SAFETY: the test passes a live `Vec<String>` it does not touch during the call.
    unsafe { &mut *user_data.cast::<Vec<String>>() }.push(args.into_owned());
}

#[test]
fn weft_generated_screen_shows_and_takes_input() {
    run_on_test_ui_thread(|| {
        let path = CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/weft/login.slint"
        ))
        .unwrap_or_default();
        // SAFETY: `path` is NUL-terminated; a null source reads the file.
        let definition = unsafe { sb_compile(ptr::null(), path.as_ptr()) };
        assert!(!definition.is_null(), "sb_compile failed: {}", last_error());
        // SAFETY: `definition` is live until the free right after.
        let host = Host(unsafe { sb_component_new(definition, WEFT_WIDTH, WEFT_HEIGHT, 1.0) });
        // SAFETY: from `sb_compile`, freed once.
        unsafe { sb_definition_free(definition) };
        assert!(
            !host.0.is_null(),
            "sb_component_new failed: {}",
            last_error()
        );

        let (mut width, mut height) = (0, 0);
        // SAFETY: `host` is live; both pointers are writable.
        assert!(unsafe { sb_host_pixel_size(host.0, &raw mut width, &raw mut height) });
        assert_eq!((width, height), (WEFT_WIDTH, WEFT_HEIGHT));

        let (frame, buf) = render(&host);
        assert_eq!(frame.redrawn, 1);
        let first = &buf[..4];
        assert!(
            buf.chunks(4).any(|px| px != first),
            "the screen is one colour"
        );

        let mut performed: Vec<String> = Vec::new();
        let name = CString::new("perform").unwrap_or_default();
        // SAFETY: `host` is live; `performed` outlives every callback below.
        let ok = unsafe {
            sb_on_callback(
                host.0,
                name.as_ptr(),
                Some(on_args),
                (&raw mut performed).cast(),
            )
        };
        assert!(ok, "{}", last_error());

        // The submit button is disabled while the email is empty.
        click(&host, WEFT_SUBMIT.0, WEFT_SUBMIT.1);
        assert!(performed.is_empty());

        // SAFETY: `host` is live.
        unsafe { assert!(sb_host_focus_changed(host.0, true)) }
        click(&host, WEFT_EMAIL.0, WEFT_EMAIL.1);
        type_ascii(&host, "ada@example.com");
        assert_eq!(get(&host, "email").as_deref(), Some("\"ada@example.com\""));
        let (frame, _) = render(&host);
        assert_eq!(frame.redrawn, 1);

        click(&host, WEFT_SUBMIT.0, WEFT_SUBMIT.1);
        // Enter in the field submits the form too (`accepted`).
        click(&host, WEFT_EMAIL.0, WEFT_EMAIL.1);
        type_ascii(&host, "\n");
        click(&host, WEFT_SIGNUP.0, WEFT_SIGNUP.1);
        assert_eq!(
            performed,
            [
                r#"["auth.submit","form"]"#,
                r#"["auth.submit","form"]"#,
                r#"["nav.signup","signup"]"#
            ]
        );
    });
}

#[test]
fn pixel_size_refuses_null_arguments() {
    run_on_test_ui_thread(|| {
        let host = open(FORM_WIDTH, FORM_HEIGHT, 1.0);
        let mut width = 0;
        // SAFETY: null host and null out-pointers are what the call must refuse.
        unsafe {
            assert!(!sb_host_pixel_size(
                ptr::null(),
                &raw mut width,
                &raw mut width
            ));
            assert!(last_error().contains("null host"), "{}", last_error());
            assert!(!sb_host_pixel_size(host.0, ptr::null_mut(), &raw mut width));
            assert!(last_error().contains("null size"), "{}", last_error());
        }
    });
}
