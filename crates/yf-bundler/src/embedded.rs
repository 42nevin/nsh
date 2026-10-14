//! JS packages compiled into the binary.

/// Module specifier of the yf runtime, imported by apps and by compiled JSX.
pub(crate) const RUNTIME_SPECIFIER: &str = "yf";

/// An embedded module: virtual id, source, and whether it is TypeScript.
pub(crate) struct Embedded {
    pub(crate) id: &'static str,
    pub(crate) source: &'static str,
    pub(crate) typescript: bool,
}

const SOLID: Embedded = Embedded {
    id: "\0yf:solid-js",
    source: include_str!("../../../js/vendor/solid-js/solid.js"),
    typescript: false,
};

const SOLID_UNIVERSAL: Embedded = Embedded {
    id: "\0yf:solid-js/universal",
    source: include_str!("../../../js/vendor/solid-js/universal.js"),
    typescript: false,
};

const RUNTIME: Embedded = Embedded {
    id: "\0yf:runtime",
    source: include_str!("../../../js/runtime/index.ts"),
    typescript: true,
};

/// Resolves a bare specifier to an embedded module.
pub(crate) fn resolve(specifier: &str) -> Option<&'static Embedded> {
    match specifier {
        "solid-js" => Some(&SOLID),
        "solid-js/universal" => Some(&SOLID_UNIVERSAL),
        RUNTIME_SPECIFIER => Some(&RUNTIME),
        _ => None,
    }
}

/// Looks up an embedded module by its virtual id.
pub(crate) fn load(id: &str) -> Option<&'static Embedded> {
    [&SOLID, &SOLID_UNIVERSAL, &RUNTIME]
        .into_iter()
        .find(|m| m.id == id)
}
