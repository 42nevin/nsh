//! Precompiles `src/web.js` to QuickJS bytecode: parsing it at startup would
//! cost about as much as the rest of the host setup combined.

use rquickjs::{
    Context, Module, Runtime,
    module::{WriteOptions, WriteOptionsEndianness},
};

fn main() {
    println!("cargo::rerun-if-changed=src/web.js");
    // Bytecode is written for 64-bit little-endian engines, like app bundles.
    let width = std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap();
    let endian = std::env::var("CARGO_CFG_TARGET_ENDIAN").unwrap();
    assert!(
        width == "64" && endian == "little",
        "nsh only supports 64-bit little-endian targets"
    );
    let source = std::fs::read_to_string("src/web.js").unwrap();
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();
    let bytes = ctx.with(|ctx| {
        let module = match Module::declare(ctx.clone(), "nsh:web", source) {
            Ok(module) => module,
            Err(err) => panic!("web.js: {err}: {:?}", ctx.catch()),
        };
        module
            .write(WriteOptions {
                endianness: WriteOptionsEndianness::Little,
                strip_source: true,
                strip_debug: false,
                ..WriteOptions::default()
            })
            .unwrap()
    });
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("web.qbc");
    std::fs::write(out, bytes).unwrap();
}
