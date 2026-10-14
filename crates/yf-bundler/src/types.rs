//! TypeScript declarations for editors: written to `.yf/types` next to an
//! app, and found through the app's `tsconfig.json` ([`TSCONFIG`]). Apps
//! need no `node_modules` for them, like for the code.

use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    isolated_declarations::{IsolatedDeclarations, IsolatedDeclarationsOptions},
    parser::Parser,
    span::SourceType,
};

/// `tsconfig.json` of a new app (`yf init`): resolves `yf` and `solid-js`
/// to `.yf/types`, and JSX to yf's elements.
pub const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2023",
    "lib": ["ES2023"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "preserve",
    "jsxImportSource": "yf",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": [],
    "paths": {
      "yf": ["./.yf/types/yf/index.d.ts"],
      "yf/jsx-runtime": ["./.yf/types/yf/jsx-runtime.d.ts"],
      "solid-js": ["./.yf/types/solid-js/types/index.d.ts"],
      "solid-js/universal": ["./.yf/types/solid-js/types/universal/index.d.ts"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx", ".yf/types/yf/globals.d.ts"]
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
    // Solid's own JSX namespace describes the DOM: yf's replaces it.
    (
        "solid-js/types/jsx.d.ts",
        "export type { JSX } from \"yf\";\n",
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

/// The files of `.yf/types`, as (path relative to it, contents).
pub fn type_definitions() -> Vec<(&'static str, String)> {
    let mut files = vec![
        (
            "yf/index.d.ts",
            declarations(crate::embedded::RUNTIME_SOURCE),
        ),
        (
            "yf/types.d.ts",
            declarations(include_str!("../../../js/runtime/types.ts")),
        ),
        (
            "yf/jsx-runtime.d.ts",
            "export type { JSX } from \"./types\";\n".to_owned(),
        ),
        (
            "yf/globals.d.ts",
            include_str!("../../../js/runtime/globals.d.ts").to_owned(),
        ),
    ];
    files.extend(
        SOLID_TYPES
            .iter()
            .map(|(path, text)| (*path, (*text).to_owned())),
    );
    files
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_declarations_need_no_inference() {
        for source in [
            crate::embedded::RUNTIME_SOURCE,
            include_str!("../../../js/runtime/types.ts"),
        ] {
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
                .any(|(p, t)| *p == "yf/index.d.ts" && t.contains("export declare function List"))
        );
    }
}
