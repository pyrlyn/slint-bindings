//! Components compiled from `.slint` source at run time.
//!
//! The interpreter and its JSON value bridge are slint_dart's
//! `slint-dart-interpreter` (shared with `flutter/`). This module only adapts
//! them to the hosts: a compiled definition goes into [`EmbeddedHost`] or
//! [`crate::GpuHost`] unchanged, because `ComponentInstance` is a
//! `ComponentHandle` like any compiled component.
//!
//! [`EmbeddedHost`]: crate::EmbeddedHost

use std::path::Path;

use slint::PlatformError;
use slint_dart_interpreter::{Definition, Engine};

pub use slint_dart_interpreter::{Instance, JsonCallback};
pub use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};

use crate::Error;

/// Compiles `.slint` source and returns its main component: the last
/// exported one, the component `slint-viewer` would show.
///
/// `path` names the file for diagnostics and resolves relative imports. When
/// `source` is `None`, the file at `path` is read.
pub fn compile(source: Option<&str>, path: &Path) -> Result<ComponentDefinition, Error> {
    let read;
    let source = match source {
        Some(source) => source,
        None => {
            read = std::fs::read_to_string(path).map_err(|source| Error::Read {
                path: path.to_owned(),
                source,
            })?;
            &read
        }
    };
    let definitions = Engine::new()
        .compile(source, &path.to_string_lossy())
        .map_err(Error::Compile)?;
    // The engine does not keep source order, so pick the component whose
    // declaration comes last in `source`; ties (e.g. imported re-exports) keep
    // the engine's last one.
    definitions
        .into_iter()
        .map(|Definition(definition)| definition)
        .max_by_key(|definition| declared_at(source, &definition.name()))
        .ok_or_else(|| Error::Compile("no exported component".to_owned()))
}

/// Byte offset of the last `component <name>` declaration in `source`.
fn declared_at(source: &str, name: &str) -> Option<usize> {
    let needle = format!("component {name}");
    source
        .match_indices(&needle)
        .filter(|(at, _)| {
            let end = at + needle.len();
            !source[end..].starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '-')
        })
        .map(|(at, _)| at)
        .last()
}

/// Creates a component of `definition` inside the host that `open` builds and
/// returns that host together with the component's JSON bridge.
///
/// `open` is a host constructor such as `|make| EmbeddedHost::new(make, w, h, scale)`;
/// it must call `make` once, which is what both hosts do.
pub fn instantiate<H>(
    definition: &ComponentDefinition,
    open: impl FnOnce(&mut dyn FnMut() -> Result<ComponentInstance, PlatformError>) -> Result<H, Error>,
) -> Result<(H, Instance), Error> {
    let definition = Definition(definition.clone());
    let mut made = None;
    let host = open(&mut || {
        let instance = definition.instantiate().map_err(PlatformError::Other)?;
        // The host owns one strong handle and the bridge another; the
        // component lives until both are dropped.
        let handle = slint::ComponentHandle::clone_strong(&instance.inner);
        made = Some(instance);
        Ok(handle)
    })?;
    let instance = made.ok_or(Error::InvalidArgument(
        "the host constructor never created the component",
    ))?;
    Ok((host, instance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddedHost;

    const SOURCE: &str = r#"
export component Counter inherits Window {
    in-out property <int> count;
    in-out property <[string]> tags;
    callback bumped(int, string);
    callback bump(int) -> int;
    bump(by) => {
        count += by;
        bumped(count, "x");
        return count;
    }
}
"#;

    fn counter() -> Result<(EmbeddedHost<ComponentInstance>, Instance), Error> {
        let definition = compile(Some(SOURCE), Path::new("counter.slint"))?;
        instantiate(&definition, |make| EmbeddedHost::new(make, 32, 32, 1.0))
    }

    #[test]
    fn interpreted_component_is_hosted_and_renders() -> Result<(), Error> {
        let (mut host, _bridge) = counter()?;
        let mut buf = vec![0u8; host.frame_len()];
        assert!(host.render(&mut buf)?.redrawn);
        Ok(())
    }

    #[test]
    fn json_bridge_reaches_the_hosted_component() -> Result<(), Error> {
        let (host, bridge) = counter()?;
        bridge
            .set_property_json("tags", r#"["a","b"]"#)
            .map_err(Error::Component)?;
        assert_eq!(
            bridge.get_property_json("tags").map_err(Error::Component)?,
            r#"["a","b"]"#
        );
        let seen = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let sink = seen.clone();
        bridge
            .set_callback_json(
                "bumped",
                Box::new(move |args| {
                    *sink.borrow_mut() = args.to_owned();
                    None
                }),
            )
            .map_err(Error::Component)?;
        assert_eq!(
            bridge
                .invoke_json("bump", "[2]")
                .map_err(Error::Component)?,
            "2"
        );
        assert_eq!(seen.borrow().as_str(), r#"[2,"x"]"#);
        // Same component on both handles.
        assert_eq!(
            host.component().get_property("count").ok(),
            Some(slint_interpreter::Value::Number(2.0))
        );
        Ok(())
    }

    #[test]
    fn compile_errors_carry_the_diagnostics() {
        let err = compile(
            Some("export component X { foo bar }"),
            Path::new("bad.slint"),
        );
        assert!(matches!(err, Err(Error::Compile(text)) if text.contains("bad.slint")));
    }

    #[test]
    fn missing_file_is_a_read_error() {
        let err = compile(None, Path::new("/nonexistent/demo.slint"));
        assert!(matches!(err, Err(Error::Read { .. })));
    }

    #[test]
    fn the_last_exported_component_is_the_main_one() -> Result<(), Error> {
        let source = "export component A inherits Window {}\nexport component B inherits Window {}";
        assert_eq!(compile(Some(source), Path::new("ab.slint"))?.name(), "B");
        Ok(())
    }
}
