//! JS packages compiled into the binary.

/// Module specifier of the renderer, imported by compiled JSX.
pub(crate) const RENDERER_SPECIFIER: &str = "nsh/renderer";

/// The language of an embedded module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    Js,
    Ts,
    /// TypeScript with JSX (the UI sets' components), compiled like app
    /// code.
    Tsx,
}

/// An embedded module: virtual id, source and language.
pub(crate) struct Embedded {
    pub(crate) id: &'static str,
    pub(crate) source: &'static str,
    pub(crate) lang: Lang,
}

/// The language of `path` by its extension (`.tsx`, else TypeScript).
const fn lang_of(path: &str) -> Lang {
    let b = path.as_bytes();
    let n = b.len();
    if n > 4 && b[n - 4] == b'.' && b[n - 3] == b't' && b[n - 2] == b's' && b[n - 1] == b'x' {
        Lang::Tsx
    } else {
        Lang::Ts
    }
}

/// Virtual id of solid-js.
pub(crate) const SOLID_ID: &str = "\0nsh:solid-js";

const SOLID: Embedded = Embedded {
    id: SOLID_ID,
    source: include_str!("../../../js/vendor/solid-js/solid.js"),
    lang: Lang::Js,
};

const SOLID_UNIVERSAL: Embedded = Embedded {
    id: "\0nsh:solid-js/universal",
    source: include_str!("../../../js/vendor/solid-js/universal.js"),
    lang: Lang::Js,
};

/// A file of nsh's JS (`js/<root>/<path>`) and the specifier apps import
/// it by (`None`: internal, imported by other embedded files only).
pub(crate) struct EmbeddedFile {
    /// Its directory under `js/` (`runtime`, `ui/mobile`).
    pub(crate) root: &'static str,
    /// Its path in the root, with the extension (`list.ts`,
    /// `components/button.tsx`).
    pub(crate) path: &'static str,
    /// `nsh`, `nsh/<module>` or `nsh/ui/<set>`.
    pub(crate) specifier: Option<&'static str>,
    pub(crate) module: Embedded,
}

impl EmbeddedFile {
    /// Its path without the extension, as relative imports name it.
    pub(crate) fn file(&self) -> &'static str {
        self.path
            .strip_suffix(".tsx")
            .or_else(|| self.path.strip_suffix(".ts"))
            .unwrap_or(self.path)
    }
}

macro_rules! embedded {
    ($root:literal: $($path:literal => $specifier:expr),* $(,)?) => {
        &[$(EmbeddedFile {
            root: $root,
            path: $path,
            specifier: $specifier,
            module: Embedded {
                id: concat!("\0nsh:", $root, "/", $path),
                source: include_str!(concat!("../../../js/", $root, "/", $path)),
                lang: lang_of($path),
            },
        }),*]
    };
}

/// The runtime (`js/runtime`): the primitives every UI set builds on, one
/// module per feature, platform-only features in `nsh/ios` and
/// `nsh/android`.
const RUNTIME: &[EmbeddedFile] = embedded! { "runtime":
    "index.ts" => Some("nsh"),
    "renderer.ts" => Some(RENDERER_SPECIFIER),
    "animated.ts" => Some("nsh/animated"),
    "device.ts" => Some("nsh/device"),
    "accessibility.ts" => Some("nsh/accessibility"),
    "list.ts" => Some("nsh/list"),
    "canvas.ts" => Some("nsh/canvas"),
    "image.ts" => Some("nsh/image"),
    "gesture.ts" => Some("nsh/gesture"),
    "webview.ts" => Some("nsh/webview"),
    "video.ts" => Some("nsh/video"),
    "native.ts" => Some("nsh/native"),
    "ios.ts" => Some("nsh/ios"),
    "android.ts" => Some("nsh/android"),
    "_core.ts" => None,
    "types.ts" => None,
};

/// The mobile UI (`js/ui/mobile`, `nsh/ui/mobile`); its components in
/// `components/` (TSX).
const UI_MOBILE: &[EmbeddedFile] = embedded! { "ui/mobile":
    "index.ts" => Some("nsh/ui/mobile"),
    "components/button.tsx" => None,
    "keyboard.ts" => None,
    "navigation.ts" => None,
    "safe-area.ts" => None,
    "system.ts" => None,
};

/// Every embedded file of nsh's JS.
pub(crate) fn files() -> impl Iterator<Item = &'static EmbeddedFile> {
    RUNTIME.iter().chain(UI_MOBILE)
}

/// The internals of the runtime, which the UI sets build on: importable by
/// embedded files only, never by apps.
const INTERNAL: &str = "nsh/_core";

/// The root of the embedded file with virtual id `id`.
fn root_of(id: &str) -> Option<&str> {
    id.strip_prefix("\0nsh:")?
        .rsplit_once('/')
        .map(|(root, _)| root)
}

/// Resolves a specifier to an embedded module: a bare one, or one relative
/// to an embedded file (`./_core` from `\0nsh:runtime/list.ts`, within its
/// root).
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
        let file = file
            .strip_suffix(".tsx")
            .or_else(|| file.strip_suffix(".ts"))
            .unwrap_or(file);
        return files()
            .find(|f| f.root == root && f.file() == file)
            .map(|f| &f.module);
    }
    if specifier == INTERNAL && embedded_root.is_some() {
        return RUNTIME
            .iter()
            .find(|f| f.file() == "_core")
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
            Some("\0nsh:runtime/index.ts")
        );
        assert_eq!(
            resolve("nsh/list", None).map(|m| m.id),
            Some("\0nsh:runtime/list.ts")
        );
        // Internal files are only reachable from nsh's own files.
        assert!(resolve("nsh/_core", None).is_none());
        assert!(resolve("nsh/_core", Some("/app/App.tsx")).is_none());
        assert!(resolve("./_core", Some("/app/App.tsx")).is_none());
        assert_eq!(
            resolve("./_core", Some("\0nsh:runtime/list.ts")).map(|m| m.id),
            Some("\0nsh:runtime/_core.ts")
        );
        // The UI sets: their own files relatively, the runtime's internals.
        assert_eq!(
            resolve("nsh/ui/mobile", None).map(|m| m.id),
            Some("\0nsh:ui/mobile/index.ts")
        );
        assert_eq!(
            resolve("./navigation", Some("\0nsh:ui/mobile/index.ts")).map(|m| m.id),
            Some("\0nsh:ui/mobile/navigation.ts")
        );
        assert!(resolve("./navigation", Some("\0nsh:runtime/index.ts")).is_none());
        assert_eq!(
            resolve("nsh/_core", Some("\0nsh:ui/mobile/navigation.ts")).map(|m| m.id),
            Some("\0nsh:runtime/_core.ts")
        );
        assert!(resolve("nsh/navigation", None).is_none());
    }
}
