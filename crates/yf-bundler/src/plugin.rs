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

use crate::embedded::{self, RUNTIME_SPECIFIER};

/// Serves embedded packages and compiles JSX with the Solid universal
/// compiler.
#[derive(Debug)]
pub(crate) struct YfPlugin {
    /// Paths in error messages are shown relative to this directory.
    pub(crate) root: PathBuf,
    /// The first compile error, already formatted. Rolldown wraps plugin
    /// errors in its own report; the bundler shows this one instead.
    pub(crate) error: Arc<Mutex<Option<String>>>,
    /// Asset files imported by the app (absolute paths).
    pub(crate) assets: Arc<Mutex<Vec<PathBuf>>>,
}

/// File types imported as image assets.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
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

impl YfPlugin {
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

impl Plugin for YfPlugin {
    fn name(&self) -> Cow<'static, str> {
        "yf".into()
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load | HookUsage::Transform
    }

    async fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        Ok(embedded::resolve(args.specifier).map(|m| HookResolveIdOutput::from_id(m.id)))
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        if let Some(m) = embedded::load(args.id) {
            return Ok(Some(HookLoadOutput {
                code: m.source.into(),
                module_type: Some(if m.typescript {
                    ModuleType::Ts
                } else {
                    ModuleType::Js
                }),
                ..Default::default()
            }));
        }
        let path = Path::new(args.id);
        if !is_image(path) {
            return Ok(None);
        }
        // An image import is an asset descriptor; the natural size (in
        // points) lets the image lay out before it is decoded.
        let dim = imagesize::size(path)
            .map_err(|e| self.fail(path, &format!("cannot read image: {e}")))?;
        let scale = density(path);
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
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
        if !matches!(args.module_type, ModuleType::Tsx | ModuleType::Jsx) {
            return Ok(None);
        }
        let path = Path::new(args.id);
        let (jsx, jsx_map) =
            strip_types(path, args.code).map_err(|detail| self.fail(path, &detail))?;
        let out = compile(
            &jsx,
            &CompileOptions {
                filename: Some(args.id.to_owned()),
                module_name: RUNTIME_SPECIFIER.into(),
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
/// compiler, which only accepts JavaScript. Errors are rendered with a code
/// frame.
fn strip_types(path: &Path, source: &str) -> Result<(String, SourceMap<'static>), String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).map_err(|e| format!("{e:?}"))?;
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
