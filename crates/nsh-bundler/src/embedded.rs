//! JS packages compiled into the binary.

/// Module specifier of the renderer, imported by compiled JSX.
pub(crate) const RENDERER_SPECIFIER: &str = "nsh/renderer";

/// An embedded module: virtual id, source, and whether it is TypeScript.
pub(crate) struct Embedded {
    pub(crate) id: &'static str,
    pub(crate) source: &'static str,
    pub(crate) typescript: bool,
}

/// Virtual id of solid-js.
pub(crate) const SOLID_ID: &str = "\0nsh:solid-js";

const SOLID: Embedded = Embedded {
    id: SOLID_ID,
    source: include_str!("../../../js/vendor/solid-js/solid.js"),
    typescript: false,
};

const SOLID_UNIVERSAL: Embedded = Embedded {
    id: "\0nsh:solid-js/universal",
    source: include_str!("../../../js/vendor/solid-js/universal.js"),
    typescript: false,
};

/// A file of the nsh runtime (`js/runtime/<file>.ts`) and the specifier
/// apps import it by (`None`: internal, only imported by other runtime
/// files).
pub(crate) struct RuntimeFile {
    /// File name without `.ts`.
    pub(crate) file: &'static str,
    /// `nsh` or `nsh/<module>`.
    pub(crate) specifier: Option<&'static str>,
    pub(crate) module: Embedded,
}

macro_rules! runtime {
    ($($file:literal => $specifier:expr),* $(,)?) => {
        &[$(RuntimeFile {
            file: $file,
            specifier: $specifier,
            module: Embedded {
                id: concat!("\0nsh:runtime/", $file),
                source: include_str!(concat!("../../../js/runtime/", $file, ".ts")),
                typescript: true,
            },
        }),*]
    };
}

/// The runtime's files: one module per feature, platform-only features in
/// `nsh/ios` and `nsh/android`.
pub(crate) const RUNTIME: &[RuntimeFile] = runtime! {
    "index" => Some("nsh"),
    "renderer" => Some(RENDERER_SPECIFIER),
    "animated" => Some("nsh/animated"),
    "device" => Some("nsh/device"),
    "keyboard" => Some("nsh/keyboard"),
    "safe-area" => Some("nsh/safe-area"),
    "accessibility" => Some("nsh/accessibility"),
    "navigation" => Some("nsh/navigation"),
    "list" => Some("nsh/list"),
    "canvas" => Some("nsh/canvas"),
    "image" => Some("nsh/image"),
    "gesture" => Some("nsh/gesture"),
    "system" => Some("nsh/system"),
    "webview" => Some("nsh/webview"),
    "video" => Some("nsh/video"),
    "ios" => Some("nsh/ios"),
    "android" => Some("nsh/android"),
    "_core" => None,
    "types" => None,
};

/// Resolves a specifier to an embedded module: a bare one, or one relative
/// to a runtime file (`./_core` from `\0nsh:runtime/list`).
pub(crate) fn resolve(specifier: &str, importer: Option<&str>) -> Option<&'static Embedded> {
    match specifier {
        "solid-js" => return Some(&SOLID),
        "solid-js/universal" => return Some(&SOLID_UNIVERSAL),
        _ => {}
    }
    if let Some(file) = specifier.strip_prefix("./")
        && importer.is_some_and(|i| i.starts_with("\0nsh:runtime/"))
    {
        let file = file.strip_suffix(".ts").unwrap_or(file);
        return RUNTIME.iter().find(|f| f.file == file).map(|f| &f.module);
    }
    RUNTIME
        .iter()
        .find(|f| f.specifier == Some(specifier))
        .map(|f| &f.module)
}

/// Looks up an embedded module by its virtual id.
pub(crate) fn load(id: &str) -> Option<&'static Embedded> {
    [&SOLID, &SOLID_UNIVERSAL]
        .into_iter()
        .chain(RUNTIME.iter().map(|f| &f.module))
        .find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_modules_resolve() {
        assert_eq!(
            resolve("nsh", None).map(|m| m.id),
            Some("\0nsh:runtime/index")
        );
        assert_eq!(
            resolve("nsh/list", None).map(|m| m.id),
            Some("\0nsh:runtime/list")
        );
        // Internal files are only reachable from the runtime itself.
        assert!(resolve("nsh/_core", None).is_none());
        assert!(resolve("./_core", Some("/app/App.tsx")).is_none());
        assert_eq!(
            resolve("./_core", Some("\0nsh:runtime/list")).map(|m| m.id),
            Some("\0nsh:runtime/_core")
        );
    }
}
