use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use dom_expressions_compiler::{CompileOptions, Generate, compile};
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

use crate::embedded::{self, RENDERER_SPECIFIER};

/// Serves embedded packages and compiles JSX with the Solid universal
/// compiler.
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
    /// The app's Tailwind theme (`nsh.json`).
    pub(crate) tailwind: Arc<TailwindSetup>,
}

/// The `"tailwind"` object of the app's `nsh.json`.
#[derive(Debug, Default)]
pub(crate) struct TailwindSetup {
    /// As JSON, for the runtime (`None`: the default theme).
    pub(crate) json: Option<String>,
    /// Parsed, to check classes at build time.
    pub(crate) theme: nsh_core::tailwind::Theme,
}

/// Virtual module with the app's theme, imported by the runtime.
const THEME_SPECIFIER: &str = "nsh:tailwind-theme";
const THEME_ID: &str = "\0nsh:tailwind-theme";

/// File types imported as image assets.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

/// File types imported as fonts.
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

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

impl NshPlugin {
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
        if args.specifier == THEME_SPECIFIER {
            return Ok(Some(HookResolveIdOutput::from_id(THEME_ID)));
        }
        Ok(embedded::resolve(args.specifier, args.importer)
            .map(|m| HookResolveIdOutput::from_id(m.id)))
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        if args.id == THEME_ID {
            let code = match &self.tailwind.json {
                Some(json) => format!(
                    "export default {};",
                    serde_json::Value::String(json.clone())
                ),
                None => "export default null;".to_owned(),
            };
            return Ok(Some(HookLoadOutput {
                code: code.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }
        if let Some(m) = embedded::load(args.id) {
            return Ok(Some(HookLoadOutput {
                code: m.source.into(),
                module_type: Some(match m.lang {
                    embedded::Lang::Js => ModuleType::Js,
                    embedded::Lang::Ts => ModuleType::Ts,
                    embedded::Lang::Tsx => ModuleType::Tsx,
                }),
                ..Default::default()
            }));
        }
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
                "export default __nsh_font({uri:?}, {family:?});",
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
        let app_module = !args.id.starts_with('\0');
        let script = matches!(args.module_type, ModuleType::Ts | ModuleType::Js);
        if !(jsx_module || self.hot && app_module && script) {
            return Ok(None);
        }
        let path = Path::new(args.id);
        let hot_key = (self.hot && app_module).then(|| {
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
        if jsx_module && app_module {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .display()
                .to_string();
            for u in crate::classes::check(args.code, source_type, &self.tailwind.theme) {
                log::warn!(target: "nsh::build", "{rel}:{}:{}: unknown class `{}`", u.line, u.column, u.class);
            }
        }
        let (jsx, jsx_map) = strip_types(path, source_type, args.code, hot_key.as_deref())
            .map_err(|detail| self.fail(path, &detail))?;
        if !jsx_module {
            // A hot TS / JS module: contexts only, no JSX to compile.
            return Ok(Some(HookTransformOutput {
                code: Some(jsx),
                map: jsx_map.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }
        let out = compile(
            &jsx,
            &CompileOptions {
                filename: Some(args.id.to_owned()),
                module_name: RENDERER_SPECIFIER.into(),
                generate: Generate::Universal,
                source_map: true,
                ..CompileOptions::default()
            },
        )
        .map_err(|e| self.fail(path, &e.to_string()))?;
        // Two steps (TSX -> JSX -> JS), one map for rolldown.
        let map = out
            .source_map
            .as_deref()
            .and_then(|json| SourceMap::from_json_string(json).ok())
            .map(|solid_map| rolldown_sourcemap::collapse_sourcemaps(&[&jsx_map, &solid_map]));
        Ok(Some(HookTransformOutput {
            code: Some(out.code),
            map: map.map(Into::into).unwrap_or_default(),
            module_type: Some(ModuleType::Js),
            ..Default::default()
        }))
    }
}

/// TSX -> JSX: removes TypeScript syntax and keeps JSX for the Solid
/// compiler, which only accepts JavaScript, and with `hot_key` makes the
/// module's components hot ([`crate::hot`]). Errors are rendered with a
/// code frame.
fn strip_types(
    path: &Path,
    source_type: SourceType,
    source: &str,
    hot_key: Option<&str>,
) -> Result<(String, SourceMap<'static>), String> {
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
        ..TransformOptions::default()
    };
    let ret =
        Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(err) = ret.diagnostics.into_iter().next() {
        return Err(err.render_with_source_code(source.to_owned()));
    }
    if let Some(key) = hot_key {
        crate::hot::transform(&allocator, &mut program, source, key);
    }
    let out = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: Some(path.to_path_buf()),
            ..CodegenOptions::default()
        })
        .build(&program);
    Ok((
        out.code,
        out.map.expect("source map requested").into_owned(),
    ))
}
