use rquickjs::{
    CatchResultExt, Context, Module, Runtime,
    module::{WriteOptions, WriteOptionsEndianness},
};

use crate::error::JsError;

/// Identifies the bytecode format produced by [`compile_bytecode`] and
/// accepted by [`crate::Host::eval_bytecode`]: QuickJS bytecode is only
/// valid for the exact engine build. Bump together with the `rquickjs` pin.
pub const BYTECODE_VERSION: &str = "rquickjs-0.14.0/quickjs-ng-0.16.2/le64";

/// Module name every bundle is evaluated under; JS stack frames reference
/// it as `bundle.js:LINE:COL`.
pub const BUNDLE_NAME: &str = "bundle.js";

/// Compiles a bundle to QuickJS bytecode.
///
/// The bytecode is only valid for the exact QuickJS version linked into
/// this crate, which is why the CLI and the runtime share it.
/// `strip` removes source and debug info (smaller, but stacks lose line
/// numbers): use it for release builds only.
pub fn compile_bytecode(source: &str, strip: bool) -> Result<Vec<u8>, JsError> {
    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;
    ctx.with(|ctx| {
        let module = Module::declare(ctx.clone(), BUNDLE_NAME, source)
            .catch(&ctx)
            .map_err(JsError::caught)?;
        let options = WriteOptions {
            endianness: WriteOptionsEndianness::Little,
            strip_source: strip,
            strip_debug: strip,
            ..WriteOptions::default()
        };
        module
            .write(options)
            .map_err(|e| JsError::from_engine(&ctx, e))
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    /// Fails when the engine is upgraded without bumping `BYTECODE_VERSION`.
    #[test]
    fn bytecode_version_matches_linked_engine() {
        // SAFETY: returns a static NUL-terminated string.
        let engine = unsafe { CStr::from_ptr(rquickjs::qjs::JS_GetVersion()) }
            .to_str()
            .unwrap();
        assert!(
            BYTECODE_VERSION.contains(&format!("quickjs-ng-{engine}/")),
            "linked QuickJS-NG {engine}, BYTECODE_VERSION says {BYTECODE_VERSION}"
        );
    }
}
