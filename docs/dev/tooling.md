# Tooling: TypeScript, the dev server, runtimes

## TypeScript

- Every command that builds an app writes `.nsh/types` next to its entry (only changed files; files nsh no longer writes
  are removed). None of it is checked in: `nsh_js::type_definitions(ui_sets)` makes the declarations of everything nsh
  serves (`nsh-js` `declarations.rs`), and `nsh_bundler::type_definitions(elements)` the app's part. The files:
  `nsh/index.d.ts` (`nsh`), one per module (`nsh/list.d.ts`, `nsh/ui/mobile.d.ts`, ...), `nsh/types.d.ts` (the
  styles, the elements' props and events, `YNode`, the JSX element type; `nsh` exports them), `nsh/globals.d.ts`
  (console, timers, frames and the web APIs), and from the bundler `nsh/jsx-runtime.d.ts` (the built-in elements and
  the app's kebab-case components, rewritten after every build: `native-pipeline.md`) and `nsh/assets.d.ts` (what
  image and font imports give). Each file declares its own names; the `import type` lines between them are worked out
  from the names each uses. `nsh init` writes a `tsconfig.json` mapping `nsh` and `nsh/*` there (`jsxImportSource:
  "nsh"`, no DOM library), so editors type-check apps without `node_modules`.
- Where each declaration comes from:
  - a native module's export: the `Ts` next to it in its table (`modules::Item`; the module's own interfaces in its
    `types`);
  - a registry's components and modules: their specs (`nsh_core::ui::declarations`; a module object's type is
    `<Name>Module`, which a native module can build on: `BackHandlerModule & BackListeners`);
  - the styles, host props, events, easings and canvas methods: the core's tables (`nsh_core::js::{style, host, anim,
    canvas}::declarations`; an event's handler type is its row in `host::EVENTS`, an event no element declares is in
    `INTERNAL_EVENTS`);
  - a web class: its spec's `ClassTs` (members as in a class body; the constants come from the spec's table), the
    types its file declares in `TYPES`, a global function's `web::Function`, the other globals `host::GLOBALS`.
- The tests keep them true: every module is loaded and its exports compared with its declarations
  (`declarations_match_the_modules`), every web class's members with its spec (`web_classes_declare_their_members`),
  the globals with a host's (`globals_match_the_host`: what is declared exists, and every global the host adds to
  QuickJS-NG's is declared), every event has a type or is internal, and every imported name has one home.
- To check the declarations themselves, run `tsc -p . --skipLibCheck false` in an app.

## Dev server (nsh dev / nsh run)

- The CLI watches sources with notify and pushes every rebuild over WebSocket (`/ws`, default port 8790, the next free port if taken) to all
  connected hosts; new clients subscribe before receiving the current state, so no event is missed. Protocol: `crates/nsh-dev/src/protocol.rs`
  (JSON text frames + binary frames).
- Bytecode version negotiation: the client reports `BYTECODE_VERSION` in its handshake; on a mismatch the server sends JS source instead.
- The host's `log` output (including JS `console.*`, target `js`) is forwarded to the CLI terminal with the device name while connected, and goes
  to stderr when disconnected.
- Build failures, bundle evaluation failures and uncaught exceptions show a red overlay on the host (tap to dismiss); it disappears on the next
  successful reload.
- **Hot module replacement** (`nsh dev` / `nsh run` without `--release`): development bundles keep one CommonJS chunk per
  module (rolldown `preserveModules`, tree shaking off), each registered with the native hot runtime
  (`crates/nsh-js/src/hot.rs`, the `__nsh_hot` global that importing `nsh:hot` makes; only development bundles import
  it). The script is an ES module importing `nsh:hot` and the namespaces of the native modules the app imports, for the
  factories' `require`; an edit importing a native module the running script did not makes the host reload. After a
  rebuild the CLI compares the chunks with the last good build and publishes a patch (`ServerMessage::Patch`, protocol
  v2) before the full bundle; hosts running the patch's base apply it in place (`EngineControl::hot_update`,
  ~0.2–0.5ms) and otherwise load the bundle. The runtime runs the changed modules again, plus the importers of modules
  that do not accept their own updates, up to the entry. The bundler's hot transform (`crates/nsh-bundler/src/hot.rs`,
  before the JSX is compiled) wraps top-level components (capitalized functions whose body has JSX) in stable proxies
  keyed by module and name, with a signature of their code plus the module's other code: a proxy is a memo over a
  version signal, so instances render again only when their signature changed (or a dependency module changed) and the
  rest of the app keeps its state. Top-level `context()` results keep their identity (taking the new default). `mount`
  calls are undone when their module runs again. A component that throws reports the error and renders nothing until
  the fix. A patch that throws shows the error and the next save recovers; a patch that cannot be applied (`false`)
  makes the host reload. Verified on the iOS simulator, the Android emulator and Mac Catalyst.
- Stacks are symbolicated with source maps: bundling produces a hidden source map (the TSX→JSX→JS maps are composed with
  `rolldown_sourcemap::collapse_sourcemaps`), sent along with the bundle; the engine rewrites `bundle.js:L:C` to `App.tsx:L:C` before reporting.

## Finding the runtimes

The CLI finds prebuilt runtimes next to itself (`bin/../runtime`, the distribution layout) or, in a checkout, in
`target/<triple>/<profile>` and `target/android/<profile>[-dev]` with the CLI's own profile (`NSH_RUNTIME` overrides).
`.cargo/config.toml` sets `IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.
