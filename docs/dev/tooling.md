# Tooling: TypeScript, the dev server, runtimes

## TypeScript

- Every command that builds an app writes `.nsh/types` next to its entry (only changed files): `nsh/index.d.ts` and `nsh/types.d.ts`
  generated from the runtime sources with Oxc isolated declarations, `nsh/jsx-runtime.d.ts`, `nsh/globals.d.ts`, and solid-js's
  declarations. `nsh init` writes a `tsconfig.json` mapping `nsh`, `nsh/jsx-runtime` and `solid-js` there (`jsxImportSource: "nsh"`,
  no DOM library), so editors type-check apps without `node_modules`.
- Exports of the runtime's files need explicit types (isolated declarations cannot infer); a nsh-bundler test
  enforces it. Keep `types.ts` in sync with what `style.rs` and `protocol.rs` accept.
- To check the declarations themselves, run `tsc -p .` in an app with `skipLibCheck` off (only solid's `hydration.d.ts`
  mentions the DOM `Element`).

## Dev server (nsh dev / nsh run)

- The CLI watches sources with notify and pushes every rebuild over WebSocket (`/ws`, default port 8790, the next free port if taken) to all
  connected hosts; new clients subscribe before receiving the current state, so no event is missed. Protocol: `crates/nsh-dev/src/protocol.rs`
  (JSON text frames + binary frames).
- Bytecode version negotiation: the client reports `BYTECODE_VERSION` in its handshake; on a mismatch the server sends JS source instead.
- The host's `log` output (including JS `console.*`, target `js`) is forwarded to the CLI terminal with the device name while connected, and goes
  to stderr when disconnected.
- Build failures, bundle evaluation failures and uncaught exceptions show a red overlay on the host (tap to dismiss); it disappears on the next
  successful reload.
- **Hot module replacement** (`nsh dev` / `nsh run` without `--release`): development bundles keep one CommonJS chunk per module
  (rolldown `preserveModules`, tree shaking off so solid-js and the runtime export everything), each registered with the hot
  runtime (`js/runtime/hot.js`). After a rebuild the CLI compares the chunks with the last good build and publishes a patch
  (`ServerMessage::Patch`, protocol v2) before the full bundle; hosts running the patch's base apply it in place
  (`EngineControl::hot_update`, ~0.2–0.5ms) and otherwise load the bundle. The runtime runs the changed modules again, plus the
  importers of modules that do not accept their own updates, up to the entry. The bundler wraps top-level components (capitalized
  functions whose body has JSX) in stable proxies keyed by module and name, with a signature of their code plus the module's other
  code: instances render again only when their signature changed (or a dependency module changed), so the rest of the app keeps its
  state. Top-level `createContext()` results keep their identity. An entry that runs again replaces what it rendered. A patch that
  throws shows the error and the next save recovers; a patch that cannot be applied (`false`) makes the host reload.
- Stacks are symbolicated with source maps: bundling produces a hidden source map (the TSX→JSX→JS maps are composed with
  `rolldown_sourcemap::collapse_sourcemaps`), sent along with the bundle; the engine rewrites `bundle.js:L:C` to `App.tsx:L:C` before reporting.

## Finding the runtimes

The CLI finds prebuilt runtimes next to itself (`bin/../runtime`, the distribution layout) or, in a checkout, in
`target/<triple>/<profile>` and `target/android/<profile>[-dev]` with the CLI's own profile (`NSH_RUNTIME` overrides).
`.cargo/config.toml` sets `IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.
