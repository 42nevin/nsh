# AGENTS.md

native-shell (nsh for short: crates, the CLI, the JS module and Java package all use `nsh`) is a React Native–like
cross-platform UI framework: app code is written in TSX + SolidJS and runs in QuickJS; layout uses Taffy; everything
renders with the system's native views (UIKit / Android View), so embedding native components such as maps or web
views has no layering problems. The toolchain is pure Rust and does not need Node.js; app developers get a prebuilt
distribution (`cargo xtask dist`) and never need Rust either.

This file is the short version. The details are in `docs/dev/` (read the page of the area you change first) and in
the module docs of the code (`//!`, Javadoc), which are the reference for how a module works inside.

## Layout

```
```
crates/
  nsh-core/            Platform-independent core: shadow tree, styles (RN semantics), Taffy layout (flex + grid), text merging, list virtualization,
                       mount items; CSS values (`css.rs`: colors, lengths, shadows, gradients, filters), effects (`effects.rs`), Tailwind (`tailwind/`)
  nsh-anim/            Native animation graph (pure Rust): value / interpolate / math nodes, timing / spring / decay drivers
  nsh-js/              QuickJS host: console / timers / op bridge, op buffer decoding, bytecode compilation and loading
  nsh-engine/          Engine thread: drives nsh-js + nsh-core and hands one MountBatch per frame to the platform
  nsh-package/         Signed packages (`.nshpkg`): Ed25519 keys, release-key certificates, revocation lists, the trust store
  nsh-bundler/         Bundler: rolldown + Oxc (TS stripping) + Solid universal JSX compiler + embedded JS packages + source maps;
                       the `nsh:tailwind-theme` module (nsh.json `tailwind`) and the unknown-class check (`classes.rs`);
                       development bundles with hot module replacement (`hot.rs`); the TypeScript declarations of `.nsh/types` (`types.rs`)
  nsh-dev/             Dev tooling: dev server protocol, server (CLI, feature `server`), client (host, feature `client`)
  nsh-platform-ios/    UIKit backend (iOS / simulator / Mac Catalyst): native view mounting, CoreText text engine, input, AppView, standalone app;
                       feature `dev`: dev server client, log forwarding, error overlay; feature `embed`: the C ABI (`embed.rs`,
                       `include/nsh.h`) under the `NativeShell` Swift module (`swift/NativeShell.swift`) iOS apps embed nsh with
  nsh-platform-android/ Android backend: `libnsh.so` (JNI glue, StaticLayout text engine, batch encoding, UI-thread animator) + the Java
                       runtime in `java/com/longbridge/nsh` (NshView, Mounter, views, images, networking); feature `dev` as above
  nsh-ios-host/        Development host app (Catalyst / iOS simulator); connects to a dev server or runs a local bundle
  nsh-ios-app/         Release runner: the executable of iOS apps built by `nsh package`
  nsh-cli/             The `nsh` command line: init / build / trace / dev / run / package; keygen / certify / publish / verify / revoke
  xtask/               Maintainer tasks: `cargo xtask android` (Android runtime), `ios-embed` (the iOS SDK), `dist` (the distribution)
js/runtime/*.ts       The JS runtime, one file per module apps import (docs/dev/js-modules.md): `index.ts` is `nsh`, `list.ts` is
                      `nsh/list`, ...; `_core.ts` is internal (the Solid universal renderer, the op buffer, the environment,
                      touches, the engine's hooks), bundled by nsh-bundler
js/runtime/types.ts   Style, element-prop and JSX types (types only; also the source of `.nsh/types/nsh/types.d.ts`)
js/runtime/globals.d.ts  Declarations of the JS globals (console, timers, fetch, ...) and of asset imports
js/runtime/hot.js     Hot module replacement runtime of development bundles (module registry, component proxies)
js/ui/mobile/         The mobile UI, `nsh/ui/mobile` (`index.ts` re-exports the others): navigation, modals, safe areas,
                      the software keyboard, the status bar, system dialogs; `components/`: its TSX components (README)
js/vendor/solid-js/   Embedded solid-js (version in VERSION) and its type declarations (`types/`, `jsx.d.ts` replaced by nsh's)
docs/                 Documentation; `docs/dev/`: for developers of nsh (architecture, internals, decisions, pitfalls)
examples/             Example apps (gallery: a demo index, one module per demo; counter), embedding samples (embed-ios, embed-android)
                      and the app they run (embedded: uses a native component the samples register)
```
```

Dependency direction: `nsh-core` ← `nsh-js` ← `nsh-engine` (also ← `nsh-package`) ← `nsh-platform-*` ←
`nsh-ios-host` / `nsh-ios-app`; `nsh-cli` depends on `nsh-bundler`, `nsh-js` and `nsh-engine`. `nsh-core` must not
depend on any platform or JS crate.

## Documentation

| Changing | Read |
|---|---|
| threads, the JS → native data flow, text pipeline | `docs/dev/architecture.md` |
| JS modules, where a new API goes, platform props | `docs/dev/js-modules.md` |
| elements, touch, accessibility, the environment | `docs/dev/elements.md`, `text.md` |
| styles, Tailwind, effects | `docs/dev/styles.md` |
| scroll views, zooming, `List` | `docs/dev/scrolling-and-lists.md` |
| animation, canvas, images | `docs/dev/animation.md`, `canvas.md`, `images.md` |
| VM limits, capabilities, signed packages | `docs/dev/sandbox.md`, `signed-packages.md` |
| packaging, the iOS / Android SDKs, native components | `docs/dev/embedding.md` |
| `.nsh/types`, the dev server, hot reload | `docs/dev/tooling.md` |
| a hot path | `docs/dev/performance.md` (baselines) |
| anything platform-specific | `docs/dev/pitfalls.md` (verified constraints, by area) |
| mobile components | `js/ui/mobile/components/README.md` |

Keep them current: a change that alters documented behavior updates its page (or the module doc) in the same commit.

## Commands

```sh
```sh
cargo build -p nsh-cli                                                             # build the nsh CLI
./target/debug/nsh init my-app                                                     # new app: App.tsx, nsh.json, tsconfig.json, .nsh/types
cargo build -p nsh-ios-host --target aarch64-apple-ios-macabi                          # build the Catalyst development host (needed by nsh run)
cargo build -p nsh-ios-host --target aarch64-apple-ios-sim                             # the iOS simulator host (`nsh run --target ios`)
cargo build -p nsh-ios-app --target aarch64-apple-ios-sim                              # the release runner (`nsh package --target ios-simulator`)
./target/debug/nsh trace examples/counter/App.tsx                                  # headless run: prints mount items and the time of every commit
./target/release/nsh trace examples/gallery/Bench.tsx --scroll 3000 --taps 0       # headless scrolling (10pt per frame): per-frame list cost
./target/debug/nsh run examples/counter/App.tsx --trace                            # dev server + Catalyst host (390×844), hot reload on save
./target/debug/nsh run examples/gallery/App.tsx --target ios                       # in an iOS simulator (boots one; needs the nsh-ios-host sim build)
./target/debug/nsh run examples/gallery/App.tsx --target android                   # on an emulator / device (needs `cargo xtask android --dev`)
./target/debug/nsh package examples/gallery/App.tsx --target ios-simulator --run   # installable app (.app / .ipa / .apk)
cargo xtask android [--release] [--dev]                                            # Android runtime into target/android/<profile>[-dev]
cargo xtask dist                                                                   # the distribution in dist/nsh
./target/debug/nsh dev examples/counter/App.tsx                                    # dev server only (for simulators / devices); prints local and LAN URLs
./target/debug/nsh build examples/counter/App.tsx --release                        # writes bundle.js / bundle.qbc
./target/debug/nsh keygen keys/root && ./target/debug/nsh keygen keys/release      # signing keys (.secret / .pub)
./target/debug/nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
./target/debug/nsh publish examples/counter/App.tsx --key keys/release.secret --cert keys/release.cert --build 2
./target/debug/nsh verify out/com.example.counter-2.nshpkg --root keys/root.pub     # as a device would check it
./target/debug/nsh revoke --root keys/root.secret --sequence 1 --package <sha256> -o revocations.json

cargo test --workspace
cargo test --release -p nsh-engine --test list_bench -- --ignored --nocapture   # gallery scroll benchmark (JS + core)
cargo test --release -p nsh-core bench_ -- --ignored --nocapture                # row / cell relayout benchmarks
cargo +nightly fmt --all                                                        # .rustfmt.toml uses nightly options
cargo clippy --workspace --all-targets
cargo clippy -p nsh-platform-ios -p nsh-ios-host -p nsh-ios-app --target aarch64-apple-ios-macabi --features nsh-platform-ios/embed,nsh-platform-ios/dev
cargo xtask ios-embed                                                           # NativeShell(Dev).xcframework into dist/nsh/embed/ios
```
```

`nsh-platform-ios` only compiles for `target_os = "ios"` (iOS devices, the iOS simulator and Catalyst); on a macOS
host it is an empty crate, so always run clippy on platform code with the Catalyst target. Likewise
`nsh-platform-android` is empty off Android: check it with `cargo xtask android` (which sets up the NDK linker, `CC`,
`AR` and bindgen's sysroot) and `cargo xtask clippy-android`. `.cargo/config.toml` sets
`IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.

## Rules

- **Two JS layers.** `nsh` / `nsh/<module>` (`js/runtime`) are the primitives and the environment every UI set builds
  on; `nsh/ui/<set>` is a whole UI for one form factor (`nsh/ui/mobile`; a desktop set will be separate, never an
  abstraction over both). Shared vs one form factor decides the layer, not JS vs native. Platform-only APIs go to
  `nsh/ios` / `nsh/android`, platform-only props into `ios={{…}}` / `android={{…}}` / `desktop={{…}}`. `nsh` has no
  catch-all re-exports; moved APIs get no compatibility re-exports.
- **Primitives only.** The runtime adds capabilities, not widgets: components are drawn with the primitives in a UI
  set (`js/ui/mobile/components`) and look the same on iOS and Android; no platform-styled widgets. React Native is a
  reference for which capabilities exist, not an API to be compatible with.
- **The main thread never waits for JS.** Callbacks the system wants answered on the spot (may this text change, may
  the back button pop, may the sheet close, ...) are answered natively from props mounted beforehand; events tell JS
  afterwards (`docs/dev/elements.md`).
- **One surface / `NshView` is one engine**, with no global singletons (nsh is embedded with several instances).
- **Limits and checks are enforced in Rust, never in JS**: the JS runtime is compiled into every bundle and is as
  trusted as the app. QuickJS bytecode is trusted input (`Bundle::bytecode` is `unsafe`): it only loads from the app's
  own resources, the dev server and verified packages (`docs/dev/signed-packages.md`).
- **Keep in sync**: the op buffer opcodes (`js/runtime/_core.ts`, `animated.ts`, `canvas.ts` ↔
  `crates/nsh-js/src/protocol.rs`); `types.ts` ↔ what `style.rs` / `protocol.rs` accept; the order of `EventKind` ↔
  `Native.java`; `encode.rs` opcodes ↔ `Mounter.java`; new event payload keys ↔ the JNI event map's key list;
  `BUNDLE_NAME` / `BYTECODE_VERSION` ↔ the linked QuickJS.

## Code conventions

- Formatting: `cargo +nightly fmt --all`, configured in `.rustfmt.toml`.
- Workspace lints: `missing_docs = warn`, `unreachable_pub = deny`, `private_interfaces = deny`. New crates must set `[lints] workspace = true`.
  They cover nsh's own Rust crates only: not the Java / Swift / TS sources, and never the apps built with nsh.
  - Every public API has a doc comment.
  - Items in private modules are `pub(crate)`, never `pub`.
- Everything is written in English: code comments, docs, logs and this file. Comments explain why, not what the code says.
- Library crates only log through the `log` facade (no `eprintln!`); performance logs use target `nsh::perf`, JS logs target `js`. Binaries install the logger.
- Dependencies are declared once in the root `Cargo.toml` `[workspace.dependencies]`; crates only write `.workspace = true`.
- Every `unsafe` block has a `// SAFETY:` comment.

## Performance rules (the project's first priority)

1. **QuickJS is the slowest link: JS does as little as possible.** `js/runtime` only pushes primitive values into one reused array. Parsing,
   diffing and string processing all happen in Rust. No JSON serialization on the JS side.
2. **Cross threads in batches.** One JS turn → one `__nsh_commit` → one `Core::commit` → one `MountBatch` → one main-thread hop. Never cross FFI
   or threads per item.
3. **No needless allocation on hot paths.** Node ids are dense: index `Vec`s (`slots`), not hash maps. Internal links use slab keys. Reuse
   buffers (`take_ops`, `queue`).
4. **Do not send mount items that are not needed.** New views get no default style and no empty event mask; unchanged styles and frames are not
   sent. Check item counts with `nsh trace`.
5. **Text**: measurements are cached per constraint (`TextContext.cache`); platforms cache font objects (`FontCache`).
6. **Changes to hot paths come with before / after `nsh trace` timings and item counts**, compared with the baselines
   in `docs/dev/performance.md` (update them when they move).

## Verification after changes

1. `cargo test --workspace`, plus fmt and clippy as listed above (including the Catalyst target and `cargo xtask clippy-android`).
2. Changes to the JS runtime, engine or core: run `nsh trace` and check item counts and timings.
3. Changes to the platform layer: launch with `nsh run`, check screenshots and simulate input where needed (the terminal needs the Screen
   Recording and Accessibility permissions; while the screen is locked the frontmost app is `loginwindow` and clicks fail, but window
   screenshots still work). iOS simulator: `xcrun simctl io booted screenshot`, `xcrun simctl ui booted appearance dark`.
   Android: `adb exec-out screencap -p`, `adb shell input tap|swipe|text|keyevent` (all work with the screen locked).
