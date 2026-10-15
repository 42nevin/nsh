//! TypeScript declarations for editors: written to `.nsh/types` next to an
//! app, and found through the app's `tsconfig.json` ([`TSCONFIG`]). Apps
//! need no `node_modules` for them, like for the code.

use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    isolated_declarations::{IsolatedDeclarations, IsolatedDeclarationsOptions},
    parser::Parser,
    span::SourceType,
};

/// `tsconfig.json` of a new app (`nsh init`): resolves `nsh` and `solid-js`
/// to `.nsh/types`, and JSX to nsh's elements.
pub const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2023",
    "lib": ["ES2023"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "preserve",
    "jsxImportSource": "nsh",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": [],
    "paths": {
      "nsh": ["./.nsh/types/nsh/index.d.ts"],
      "nsh/*": ["./.nsh/types/nsh/*.d.ts"],
      "solid-js": ["./.nsh/types/solid-js/types/index.d.ts"],
      "solid-js/universal": ["./.nsh/types/solid-js/types/universal/index.d.ts"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx", ".nsh/types/nsh/globals.d.ts"]
}
"#;

const SOLID_TYPES: &[(&str, &str)] = &[
    (
        "solid-js/types/index.d.ts",
        include_str!("../../../js/vendor/solid-js/types/index.d.ts"),
    ),
    (
        "solid-js/types/reactive/signal.d.ts",
        include_str!("../../../js/vendor/solid-js/types/reactive/signal.d.ts"),
    ),
    (
        "solid-js/types/reactive/observable.d.ts",
        include_str!("../../../js/vendor/solid-js/types/reactive/observable.d.ts"),
    ),
    (
        "solid-js/types/reactive/scheduler.d.ts",
        include_str!("../../../js/vendor/solid-js/types/reactive/scheduler.d.ts"),
    ),
    (
        "solid-js/types/reactive/array.d.ts",
        include_str!("../../../js/vendor/solid-js/types/reactive/array.d.ts"),
    ),
    (
        "solid-js/types/render/index.d.ts",
        include_str!("../../../js/vendor/solid-js/types/render/index.d.ts"),
    ),
    (
        "solid-js/types/render/component.d.ts",
        include_str!("../../../js/vendor/solid-js/types/render/component.d.ts"),
    ),
    (
        "solid-js/types/render/flow.d.ts",
        include_str!("../../../js/vendor/solid-js/types/render/flow.d.ts"),
    ),
    (
        "solid-js/types/render/Suspense.d.ts",
        include_str!("../../../js/vendor/solid-js/types/render/Suspense.d.ts"),
    ),
    (
        "solid-js/types/render/hydration.d.ts",
        include_str!("../../../js/vendor/solid-js/types/render/hydration.d.ts"),
    ),
    (
        "solid-js/types/universal/index.d.ts",
        include_str!("../../../js/vendor/solid-js/types/universal/index.d.ts"),
    ),
    (
        "solid-js/types/universal/universal.d.ts",
        include_str!("../../../js/vendor/solid-js/types/universal/universal.d.ts"),
    ),
    // Solid's own JSX namespace describes the DOM: nsh's replaces it.
    (
        "solid-js/types/jsx.d.ts",
        "export type { JSX } from \"nsh\";\n",
    ),
];

/// Declarations of a TypeScript module, from its source (it must have the
/// explicit types `--isolatedDeclarations` asks for).
fn declarations(source: &str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let ret = IsolatedDeclarations::new(
        &allocator,
        IsolatedDeclarationsOptions {
            strip_internal: true,
        },
    )
    .build(&parsed.program);
    debug_assert!(
        ret.diagnostics.is_empty(),
        "the runtime needs explicit types: {:?}",
        ret.diagnostics
    );
    Codegen::new().build(&ret.program).code
}

/// The files of `.nsh/types`, as (path relative to it, contents): one
/// declaration file per runtime file (`nsh/list.d.ts` for `nsh/list`; they
/// import each other relatively, as the sources do).
pub fn type_definitions() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = crate::embedded::RUNTIME
        .iter()
        .map(|f| {
            (
                format!("nsh/{}.d.ts", f.file),
                declarations(f.module.source),
            )
        })
        .collect();
    files.extend(
        [
            (
                "nsh/jsx-runtime.d.ts",
                "export type { JSX } from \"./types\";\n".to_owned(),
            ),
            (
                "nsh/globals.d.ts",
                include_str!("../../../js/runtime/globals.d.ts").to_owned(),
            ),
        ]
        .map(|(path, text)| (path.to_owned(), text)),
    );
    files.extend(
        SOLID_TYPES
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned())),
    );
    files
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_declarations_need_no_inference() {
        for source in crate::embedded::RUNTIME.iter().map(|f| f.module.source) {
            let allocator = super::Allocator::default();
            let parsed = super::Parser::new(&allocator, source, super::SourceType::ts()).parse();
            let ret = super::IsolatedDeclarations::new(
                &allocator,
                super::IsolatedDeclarationsOptions {
                    strip_internal: true,
                },
            )
            .build(&parsed.program);
            assert!(ret.diagnostics.is_empty(), "{:?}", ret.diagnostics);
        }
        let files = super::type_definitions();
        assert!(
            files
                .iter()
                .any(|(p, t)| p == "nsh/list.d.ts" && t.contains("export declare function List"))
        );
    }
}
