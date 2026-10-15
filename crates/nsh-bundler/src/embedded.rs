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

/// A file of nsh's JS (`js/<root>/<file>.ts`) and the specifier apps
/// import it by (`None`: internal, imported by other embedded files only).
pub(crate) struct EmbeddedFile {
    /// Its directory under `js/` (`runtime`, `ui/mobile`).
    pub(crate) root: &'static str,
    /// File name without `.ts`.
    pub(crate) file: &'static str,
    /// `nsh`, `nsh/<module>` or `nsh/ui/<set>`.
    pub(crate) specifier: Option<&'static str>,
    pub(crate) module: Embedded,
}

macro_rules! embedded {
    ($root:literal: $($file:literal => $specifier:expr),* $(,)?) => {
        &[$(EmbeddedFile {
            root: $root,
            file: $file,
            specifier: $specifier,
            module: Embedded {
                id: concat!("\0nsh:", $root, "/", $file),
                source: include_str!(concat!("../../../js/", $root, "/", $file, ".ts")),
                typescript: true,
            },
        }),*]
    };
}

/// The runtime (`js/runtime`): the primitives every UI set builds on, one
/// module per feature, platform-only features in `nsh/ios` and
/// `nsh/android`.
const RUNTIME: &[EmbeddedFile] = embedded! { "runtime":
    "index" => Some("nsh"),
    "renderer" => Some(RENDERER_SPECIFIER),
    "animated" => Some("nsh/animated"),
    "device" => Some("nsh/device"),
    "accessibility" => Some("nsh/accessibility"),
    "list" => Some("nsh/list"),
    "canvas" => Some("nsh/canvas"),
    "image" => Some("nsh/image"),
    "gesture" => Some("nsh/gesture"),
    "webview" => Some("nsh/webview"),
    "video" => Some("nsh/video"),
    "ios" => Some("nsh/ios"),
    "android" => Some("nsh/android"),
    "_core" => None,
    "types" => None,
};

/// The mobile UI (`js/ui/mobile`, `nsh/ui/mobile`).
const UI_MOBILE: &[EmbeddedFile] = embedded! { "ui/mobile":
    "index" => Some("nsh/ui/mobile"),
    "keyboard" => None,
    "navigation" => None,
    "safe-area" => None,
    "system" => None,
};

/// Every embedded file of nsh's JS.
pub(crate) fn files() -> impl Iterator<Item = &'static EmbeddedFile> {
    RUNTIME.iter().chain(UI_MOBILE)
}

/// The internals of the runtime, which the UI sets build on: importable by
/// embedded files only, never by apps.
const INTERNAL: &str = "nsh/_core";

/// The root of an embedded file's virtual id (`ui/mobile` of
/// `\0nsh:ui/mobile/navigation`).
fn root_of(id: &str) -> Option<&str> {
    id.strip_prefix("\0nsh:")?
        .rsplit_once('/')
        .map(|(root, _)| root)
}

/// Resolves a specifier to an embedded module: a bare one, or one relative
/// to an embedded file (`./_core` from `\0nsh:runtime/list`, within its root).
pub(crate) fn resolve(specifier: &str, importer: Option<&str>) -> Option<&'static Embedded> {
    match specifier {
        "solid-js" => return Some(&SOLID),
        "solid-js/universal" => return Some(&SOLID_UNIVERSAL),
        _ => {}
    }
    let embedded_root = importer.and_then(root_of);
    if let Some(file) = specifier.strip_prefix("./")
        && let Some(root) = embedded_root
    {
        let file = file.strip_suffix(".ts").unwrap_or(file);
        return files()
            .find(|f| f.root == root && f.file == file)
            .map(|f| &f.module);
    }
    if specifier == INTERNAL && embedded_root.is_some() {
        return RUNTIME
            .iter()
            .find(|f| f.file == "_core")
            .map(|f| &f.module);
    }
    files()
        .find(|f| f.specifier == Some(specifier))
        .map(|f| &f.module)
}

/// Looks up an embedded module by its virtual id.
pub(crate) fn load(id: &str) -> Option<&'static Embedded> {
    [&SOLID, &SOLID_UNIVERSAL]
        .into_iter()
        .chain(files().map(|f| &f.module))
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
        // Internal files are only reachable from nsh's own files.
        assert!(resolve("nsh/_core", None).is_none());
        assert!(resolve("nsh/_core", Some("/app/App.tsx")).is_none());
        assert!(resolve("./_core", Some("/app/App.tsx")).is_none());
        assert_eq!(
            resolve("./_core", Some("\0nsh:runtime/list")).map(|m| m.id),
            Some("\0nsh:runtime/_core")
        );
        // The UI sets: their own files relatively, the runtime's internals.
        assert_eq!(
            resolve("nsh/ui/mobile", None).map(|m| m.id),
            Some("\0nsh:ui/mobile/index")
        );
        assert_eq!(
            resolve("./navigation", Some("\0nsh:ui/mobile/index")).map(|m| m.id),
            Some("\0nsh:ui/mobile/navigation")
        );
        assert!(resolve("./navigation", Some("\0nsh:runtime/index")).is_none());
        assert_eq!(
            resolve("nsh/_core", Some("\0nsh:ui/mobile/navigation")).map(|m| m.id),
            Some("\0nsh:runtime/_core")
        );
        assert!(resolve("nsh/navigation", None).is_none());
    }
}
