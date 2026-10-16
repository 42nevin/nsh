use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use oxc::{
    allocator::Allocator,
    codegen::{Codegen, CodegenOptions},
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{JsxOptions, TransformOptions, Transformer},
};
use oxc_sourcemap::SourceMap;
use rolldown::{
    ModuleType,
    plugin::{
        HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
        HookResolveIdReturn, HookTransformArgs, HookTransformOutput, HookTransformReturn,
        HookUsage, Plugin, PluginContext, SharedLoadPluginContext, SharedTransformPluginContext,
    },
};

/// Compiles the app's modules with nsh's JSX compiler against the native
/// modules (kept as imports) and loads its assets.
#[derive(Debug)]
pub(crate) struct NshPlugin {
    /// Paths in error messages are shown relative to this directory.
    pub(crate) root: PathBuf,
    /// The first compile error, already formatted. Rolldown wraps plugin
    /// errors in its own report; the bundler shows this one instead.
    pub(crate) error: Arc<Mutex<Option<String>>>,
    /// Asset files imported by the app (absolute paths).
    pub(crate) assets: Arc<Mutex<Vec<PathBuf>>>,
    /// Development bundle with hot module replacement: components and
    /// contexts of app modules are registered with the hot runtime.
    pub(crate) hot: bool,
    /// The app's themes (`nsh.json`).
    pub(crate) tailwind: Arc<ThemeSetup>,
    /// The entry module (it sets up the themes).
    pub(crate) entry: PathBuf,
    /// The components the app names in kebab case.
    pub(crate) elements: Arc<Mutex<Vec<crate::AppElement>>>,
    /// The native modules the app imports.
    pub(crate) natives: Arc<Mutex<std::collections::BTreeSet<String>>>,
}

/// The `"tailwind"` and `"theme"` of the app's `nsh.json`.
#[derive(Debug, Default)]
pub(crate) struct ThemeSetup {
    /// The Tailwind theme as JSON, for the runtime (`None`: the default).
    pub(crate) json: Option<String>,
    /// Parsed, to check classes at build time.
    pub(crate) theme: nsh_core::tailwind::Theme,
    /// The theme of the Rust components (a gpui-kit theme file), checked.
    pub(crate) ui: Option<String>,
}

/// File types imported as image assets.
pub(crate) const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

/// File types imported as fonts.
pub(crate) const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// The family name a font registers under: the typographic family (name
/// 16) when present, else the legacy family (name 1).
fn font_family(data: &[u8]) -> Result<String, String> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
    let name = |id| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
    };
    name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(ttf_parser::name_id::FAMILY))
        .ok_or_else(|| "the font has no family name".to_owned())
}

/// `logo@2x.png` -> 2.0: the pixel density encoded in the file name.
fn density(path: &Path) -> f64 {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match stem.rsplit_once('@').and_then(|(_, d)| d.strip_suffix('x')) {
        Some(d) => d.parse().unwrap_or(1.0),
        None => 1.0,
    }
}

/// Whether `spec` is a native module: nsh's own,
/// the mobile UI's or a runtime registry's.
fn native_module(spec: &str) -> bool {
    nsh_core::js::NATIVE_MODULES.contains(&spec)
        || spec == nsh_ui_mobile::SPECIFIER
        || nsh_core::runtime::registries()
            .iter()
            .any(|r| r.specifier() == Some(spec))
}

/// Whether export `name` of `spec` is one of nsh's objects of functions
/// (`Image` of `nsh/image`), not a component.
pub(crate) fn native_object(spec: &str, name: &str) -> bool {
    static UI: std::sync::OnceLock<nsh_core::ui::Registry> = std::sync::OnceLock::new();
    std::iter::once(UI.get_or_init(nsh_ui_mobile::registry))
        .chain(nsh_core::runtime::registries())
        .filter(|r| r.specifier() == Some(spec))
        .any(|r| r.module_specs().iter().any(|m| !m.flat && m.name == name))
}

/// The components of the mobile UI, for hints about tags they would be.
fn ui_components() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        nsh_ui_mobile::registry()
            .specs()
            .iter()
            .map(|s| s.export.clone())
            .collect()
    })
}

impl NshPlugin {
    /// The entry's first statement, giving the renderer the app's themes.
    fn prologue(&self) -> Option<String> {
        let json = |v: &Option<String>| match v {
            Some(json) => serde_json::Value::String(json.clone()).to_string(),
            None => "null".to_owned(),
        };
        (self.tailwind.json.is_some() || self.tailwind.ui.is_some()).then(|| {
            format!(
                "_$setup({}, {});",
                json(&self.tailwind.json),
                json(&self.tailwind.ui)
            )
        })
    }

    /// Records `message` as the build's error (shown as it is).
    fn refuse(&self, message: String) -> anyhow::Error {
        self.error
            .lock()
            .expect("error lock")
            .get_or_insert_with(|| message.clone());
        anyhow!(message)
    }

    fn fail(&self, path: &Path, detail: &str) -> anyhow::Error {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        let message = format!("{}\n{detail}", rel.display());
        self.error
            .lock()
            .expect("error lock")
            .get_or_insert_with(|| message.clone());
        anyhow!(message)
    }
}

impl Plugin for NshPlugin {
    fn name(&self) -> Cow<'static, str> {
        "nsh".into()
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load | HookUsage::Transform
    }

    async fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        let spec = args.specifier;
        if native_module(spec) {
            self.natives
                .lock()
                .expect("natives lock")
                .insert(spec.to_owned());
            return Ok(Some(HookResolveIdOutput {
                external: Some(true.into()),
                ..HookResolveIdOutput::from_id(spec)
            }));
        }
        let from = args
            .importer
            .map(|i| {
                let p = Path::new(i);
                format!("{}: ", p.strip_prefix(&self.root).unwrap_or(p).display())
            })
            .unwrap_or_default();
        if spec == "solid-js" || spec.starts_with("solid-js/") {
            return Err(self.refuse(format!(
                "{from}`{spec}` is not part of nsh apps: import signal, computed, effect, batch, untracked, cleanup, context and mount from \"nsh\""
            )));
        }
        if spec.starts_with("nsh/") || spec.starts_with("nsh:") {
            return Err(self.refuse(format!("{from}nsh has no module `{spec}`")));
        }
        Ok(None)
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let path = Path::new(args.id);
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if has_extension(path, FONT_EXTENSIONS) {
            // A font import is its family name; the runtime registers the
            // font before the first frame.
            let data = std::fs::read(path).map_err(|e| self.fail(path, &e.to_string()))?;
            let family = font_family(&data)
                .map_err(|e| self.fail(path, &format!("cannot read font: {e}")))?;
            self.assets
                .lock()
                .expect("assets lock")
                .push(path.to_path_buf());
            let code = format!(
                "import {{ font }} from \"nsh:rt\";\nexport default font({uri:?}, {family:?});",
                uri = format!("asset:/{rel}"),
            );
            return Ok(Some(HookLoadOutput {
                code: code.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }
        if !has_extension(path, IMAGE_EXTENSIONS) {
            return Ok(None);
        }
        // An image import is an asset descriptor; the natural size (in
        // points) lets the image lay out before it is decoded.
        let dim = imagesize::size(path)
            .map_err(|e| self.fail(path, &format!("cannot read image: {e}")))?;
        let scale = density(path);
        self.assets
            .lock()
            .expect("assets lock")
            .push(path.to_path_buf());
        let code = format!(
            "export default {{ uri: {uri:?}, width: {w}, height: {h} }};",
            uri = format!("asset:/{rel}"),
            w = dim.width as f64 / scale,
            h = dim.height as f64 / scale,
        );
        Ok(Some(HookLoadOutput {
            code: code.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        let jsx_module = matches!(args.module_type, ModuleType::Tsx | ModuleType::Jsx);
        // Rolldown's own helpers.
        if args.id.starts_with('\0') {
            return Ok(None);
        }
        let path = Path::new(args.id);
        let entry = path == self.entry;
        let script = matches!(args.module_type, ModuleType::Ts | ModuleType::Js);
        if !(jsx_module || entry || self.hot && script) {
            return Ok(None);
        }
        let hot_key = self.hot.then(|| {
            path.strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        });
        // By module type, not file name: asset modules (`logo.png`) are JS.
        let source_type = match args.module_type {
            ModuleType::Tsx => SourceType::tsx(),
            ModuleType::Jsx => SourceType::jsx(),
            ModuleType::Ts => SourceType::ts(),
            _ => SourceType::mjs(),
        };
        if jsx_module {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .display()
                .to_string();
            for u in crate::classes::check(args.code, source_type, &self.tailwind.theme) {
                log::warn!(target: "nsh::build", "{rel}:{}:{}: unknown class `{}`", u.line, u.column, u.class);
            }
        }
        let prologue = if entry { self.prologue() } else { None };
        let (code, map, uses) = compile_native(
            path,
            source_type,
            args.code,
            prologue.as_deref(),
            hot_key.as_deref(),
        )
        .map_err(|detail| self.fail(path, &detail))?;
        let module = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .with_extension("");
        let mut elements = self.elements.lock().expect("elements lock");
        for u in uses {
            elements.retain(|e| e.tag != u.tag);
            elements.push(crate::AppElement {
                tag: u.tag,
                source: u.source.map(|(specifier, export)| {
                    (type_module(&module, specifier.as_deref()), export)
                }),
            });
        }
        drop(elements);
        Ok(Some(HookTransformOutput {
            code: Some(code),
            map: map.into(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }
}

/// The module a component's type is imported from, as the app's types
/// name it: `./`-relative to the app directory, or a package. `module` is
/// the importing module (relative to the app, without extension);
/// `specifier` what it imported (`None`: the module itself).
fn type_module(module: &Path, specifier: Option<&str>) -> String {
    let rel = |p: &Path| {
        let mut parts: Vec<String> = Vec::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                std::path::Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
                _ => {}
            }
        }
        format!("./{}", parts.join("/"))
    };
    match specifier {
        None => rel(module),
        Some(spec) if spec.starts_with('.') => {
            let dir = module.parent().unwrap_or(Path::new(""));
            let target = dir.join(spec);
            let target = match target.extension().and_then(|e| e.to_str()) {
                Some("ts" | "tsx" | "js" | "jsx") => target.with_extension(""),
                _ => target,
            };
            rel(&target)
        }
        Some(spec) => spec.to_owned(),
    }
}

/// TSX -> JS with nsh's JSX compiler ([`crate::jsx`]), TypeScript removed
/// in the same pass; `prologue` runs first; with `hot_key` the module's
/// components are hot ([`crate::hot`]). Returns the components named in
/// kebab case too. Errors are rendered with a code frame.
pub(crate) fn compile_native(
    path: &Path,
    source_type: SourceType,
    source: &str,
    prologue: Option<&str>,
    hot_key: Option<&str>,
) -> Result<(String, SourceMap<'static>, Vec<crate::jsx::KebabUse>), String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(err) = parsed.diagnostics.into_iter().next() {
        return Err(err.render_with_source_code(source.to_owned()));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();
    let options = TransformOptions {
        jsx: JsxOptions::disable(),
        // Imports only kebab tags use (`<card>` for `Card`) look unused to
        // TypeScript: only type imports go.
        typescript: oxc::transformer::TypeScriptOptions {
            only_remove_type_imports: true,
            ..Default::default()
        },
        ..TransformOptions::default()
    };
    let ret =
        Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(err) = ret.diagnostics.into_iter().next() {
        return Err(err.render_with_source_code(source.to_owned()));
    }
    if let Some(key) = hot_key {
        // Before the JSX goes: components are the functions with JSX.
        crate::hot::transform(&allocator, &mut program, source, key);
    }
    let components: Vec<&str> = ui_components().iter().map(String::as_str).collect();
    let uses = crate::jsx::transform(
        &allocator,
        &mut program,
        &ret.scoping,
        source,
        prologue,
        &components,
    )?;
    elide_unused_imports(&mut program);
    let out = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: Some(path.to_path_buf()),
            ..CodegenOptions::default()
        })
        .build(&program);
    Ok((
        out.code,
        out.map.expect("source map requested").into_owned(),
        uses,
    ))
}

/// TypeScript's import elision, once the JSX is compiled (kebab tags now
/// reference what they name): import specifiers nothing references go (an
/// import of types only, without `type`), and declarations left without
/// any.
fn elide_unused_imports(program: &mut oxc::ast::ast::Program<'_>) {
    use oxc::ast::ast::{ImportDeclarationSpecifier as S, Statement};
    let scoping = SemanticBuilder::new()
        .build(program)
        .semantic
        .into_scoping();
    let used = |local: &oxc::ast::ast::BindingIdentifier<'_>| {
        local
            .symbol_id
            .get()
            .is_none_or(|id| !scoping.get_resolved_reference_ids(id).is_empty())
    };
    program.body.retain_mut(|statement| {
        let Statement::ImportDeclaration(import) = statement else {
            return true;
        };
        let Some(specifiers) = &mut import.specifiers else {
            // `import "x"`: for its effects.
            return true;
        };
        if specifiers.is_empty() {
            return true;
        }
        specifiers.retain(|s| match s {
            S::ImportSpecifier(s) => used(&s.local),
            S::ImportDefaultSpecifier(s) => used(&s.local),
            S::ImportNamespaceSpecifier(s) => used(&s.local),
        });
        !specifiers.is_empty()
    });
}
