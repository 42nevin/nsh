# AGENTS.md

yf is a React Native–like cross-platform UI framework: app code is written in TSX + SolidJS and runs in QuickJS; layout uses Taffy; everything renders with the system's native views (UIKit / Android View), so embedding native components such as maps or web views has no layering problems. The toolchain is pure Rust and does not need Node.js.

## Layout

```
crates/
  yf-core/            Platform-independent core: shadow tree, styles (RN semantics), Taffy layout, text merging, list virtualization, mount items
  yf-anim/            Native animation graph (pure Rust): value / interpolate / math nodes, timing / spring / decay drivers
  yf-js/              QuickJS host: console / timers / op bridge, op buffer decoding, bytecode compilation and loading
  yf-engine/          Engine thread: drives yf-js + yf-core and hands one MountBatch per frame to the platform
  yf-bundler/         Bundler: rolldown + Oxc (TS stripping) + Solid universal JSX compiler + embedded JS packages + source maps
  yf-dev/             Dev tooling: dev server protocol, server (CLI, feature `server`), client (host, feature `client`)
  yf-platform-apple/  UIKit backend (iOS / simulator / Mac Catalyst): native view mounting, CoreText text engine, input, Surface, standalone app;
                      feature `dev`: dev server client, log forwarding, error overlay (only yf-host enables it)
  yf-host/            Prebuilt development host app (Catalyst); connects to a dev server or runs a local bundle
  yf-cli/             The `yf` command line: build / trace / dev / run
js/runtime/index.ts   Solid universal renderer (JS side), bundled by yf-bundler
js/vendor/solid-js/   Embedded solid-js (version in VERSION)
examples/             Example apps
```

Dependency direction: `yf-core` ← `yf-js` ← `yf-engine` ← `yf-platform-*` ← `yf-host`; `yf-cli` depends on `yf-bundler`, `yf-js` and `yf-engine`. `yf-core` must not depend on any platform or JS crate.

## Data flow and threads

```
JS (engine thread, QuickJS)  Solid components -> js/runtime pushes changes into one flat array -> __yf_commit(ops) in a microtask
yf-js                        decodes the op buffer in Rust (tags, style objects and event names are parsed here) -> Vec<Op>
yf-core (engine thread)      apply(Op) maintains the shadow tree -> commit(): rebuild text, Taffy layout, diff frames -> MountBatch
Platform (main thread)       one main-queue hop per commit, applying MountItems to native views in order
Input                        native gestures -> EngineControl::dispatch_event -> engine thread -> JS handler
```

- The platform main thread never waits for JS.
- **Text is laid out once**: `TextEngine` (defined in core) runs `prepare` on the engine thread (Apple: CTFramesetter) → `measure` at various widths →
  `layout` at the final size after layout (Apple: CTFrame). `MountItem::Text` hands the immutable result to the main thread, which only draws it
  (`CTFrameDraw` in `TextView`'s `drawRect`); the main thread never lays out text. Android's counterpart is `StaticLayout` (like RN's PreparedLayout).
  Implementations must not touch main-thread-only UI objects.
- Measure cache: one content-addressed LRU per engine (`MeasureCache`, two generations of hash maps, ≤1024 entries), keyed by a hash of the
  layout-relevant content (`AttributedText::layout_hash`, colors excluded) + the width constraint; nodes also cache `Prepared` and the final layout.
  A color-only change does not trigger layout, only a new final layout.
- One `Surface` is one engine (one QuickJS runtime), with no global singletons, so yf can be embedded in existing apps with several instances.
  A standalone app is just "one Surface filling the window".

## Commands

```sh
cargo build -p yf-cli                                         # build the yf CLI
cargo build -p yf-host --target aarch64-apple-ios-macabi      # build the Catalyst development host (needed by yf run)
./target/debug/yf trace examples/counter/App.tsx              # headless run: prints mount items and the time of every commit
./target/release/yf trace examples/gallery/App.tsx --scroll 3000 --taps 0   # headless scrolling (10pt per frame): per-frame list cost
./target/debug/yf run examples/counter/App.tsx --trace        # dev server + Catalyst host (390×844), hot reload on save
./target/debug/yf dev examples/counter/App.tsx                # dev server only (for simulators / devices); prints local and LAN URLs
./target/debug/yf build examples/counter/App.tsx --release    # writes bundle.js / bundle.qbc

cargo test --workspace
cargo test --release -p yf-engine --test list_bench -- --ignored --nocapture   # gallery scroll benchmark (JS + core)
cargo test --release -p yf-core bench_ -- --ignored --nocapture                # row / cell relayout benchmarks
cargo +nightly fmt --all                                      # .rustfmt.toml uses nightly options
cargo clippy --workspace --all-targets
cargo clippy -p yf-platform-apple -p yf-host --target aarch64-apple-ios-macabi
```

`yf-platform-apple` only compiles for `target_os = "ios"` (iOS devices, the iOS simulator and Catalyst); on a macOS host it is an empty crate, so always run clippy on platform code with the Catalyst target.

## Elements and capabilities

| JSX | Native (Apple) | Notes |
|---|---|---|
| `<view>` | UIView | `onPress` / `onLongPress` / `pan={{x, y}}` + `onPanStart` / `onPanEnd` |
| `<text>` | `TextView` (CTFrame) | nested `<text>` is a span |
| `<image src>` | UIView + image CALayer | `src` is a URL or `import img from "./a.png"` (an asset descriptor with its size); `resizeMode`; `onLoad` / `onError` |
| `<scroll>` | UIScrollView | `horizontal`, `refreshing` + `onRefresh`, `scrollValue`, `onScrollEnd` |
| `List` (runtime component) | UIScrollView + recycled cells | `each` / `estimatedItemSize` / `onEndReached` / pull to refresh / `scrollValue` |

- **View styles**: `backgroundColor`, `borderWidth` / `borderColor`, `borderRadius` + the four `borderTopLeftRadius`-style corners, `overflow`, `opacity`,
  `transform`, `shadowColor` / `shadowOffset` / `shadowOpacity` / `shadowRadius` (RN defaults: offset `(0, -3)`, radius 3, opacity 0).
  On Apple everything is drawn by Core Animation (`decor.rs`), escalating only as needed: rounded corners sharing one radius (possibly only some
  corners) → the layer's own `cornerRadius` + `maskedCorners`; different radii → `CAShapeLayer` fill / stroke / mask; clipping + shadow → the
  children move into a content view (clipping a layer clips its shadow too). With a background the shadow gets a `shadowPath` (no offscreen
  pass); with a transparent background the shadow follows the content (e.g. text shadows).
  Known limits: scroll / list only support one radius (the largest wins) and clip their shadow; `<text>` with different radii is masked, which
  clips its shadow too; no per-side borders yet.
- **List**: the window is computed in Rust (`core/list.rs`, Fenwick prefix sums), rendering one extra screen above and below the viewport.
  Rust asks JS to bind cells with `__yf_list(listId, [slot, index, …])`; JS updates the cell content inside `batch` and then sets `cellIndex` to
  confirm, and core only measures / positions confirmed cells. New cells are created with their first index. The engine runs at most 8
  "bind → JS → layout" rounds per frame, merged into one batch. Row-local state travels with recycled cells: keep it outside, keyed by id.
- **Animation**: `createNativeValue()` → `timing` / `spring` / `decay` (return a Promise), `interpolate` (numbers or colors), `add/sub/mul/div`.
  Bindable: `opacity`, `backgroundColor`, and the translate / scale / rotate parts of `transform`. Driven on the main thread by `CADisplayLink`,
  paused while nothing animates. `scrollValue` writes the scroll offset into a native value in the same frame; `pan` lets drags drive native values
  directly, and JS receives the velocity when the drag ends. Native values referenced by a view's style are kept on the view node (`YNode.anims`)
  and derived nodes hold their inputs, so bindings do not break when JS objects are collected.
- **Images**: background NSURLSession / file read → ImageIO downsampling decode at the view's pixel size → `NSCache` (purged under memory pressure);
  requests for the same key are coalesced and downloads nobody waits for are cancelled. During development `asset:/path` goes through the dev
  server's `/assets/`; `yf build` copies assets to `out/assets/`.

## Dev server (yf dev / yf run)

- The CLI watches sources with notify and pushes every rebuild over WebSocket (`/ws`, default port 8790, the next free port if taken) to all
  connected hosts; new clients subscribe before receiving the current state, so no event is missed. Protocol: `crates/yf-dev/src/protocol.rs`
  (JSON text frames + binary frames).
- Bytecode version negotiation: the client reports `BYTECODE_VERSION` in its handshake; on a mismatch the server sends JS source instead.
- The host's `log` output (including JS `console.*`, target `js`) is forwarded to the CLI terminal with the device name while connected, and goes
  to stderr when disconnected.
- Build failures, bundle evaluation failures and uncaught exceptions show a red overlay on the host (tap to dismiss); it disappears on the next
  successful reload.
- Stacks are symbolicated with source maps: bundling produces a hidden source map (the TSX→JSX→JS maps are composed with
  `rolldown_sourcemap::collapse_sourcemaps`), sent along with the bundle; the engine rewrites `bundle.js:L:C` to `App.tsx:L:C` before reporting.

## Code conventions

- Formatting: `cargo +nightly fmt --all`, configured in `.rustfmt.toml`.
- Workspace lints: `missing_docs = warn`, `unreachable_pub = deny`, `private_interfaces = deny`. New crates must set `[lints] workspace = true`.
  - Every public API has a doc comment.
  - Items in private modules are `pub(crate)`, never `pub`.
- Everything is written in English: code comments, docs, logs and this file. Comments explain why, not what the code says.
- Library crates only log through the `log` facade (no `eprintln!`); performance logs use target `yf::perf`, JS logs target `js`. Binaries install the logger.
- Dependencies are declared once in the root `Cargo.toml` `[workspace.dependencies]`; crates only write `.workspace = true`.
- Every `unsafe` block has a `// SAFETY:` comment.

## Performance rules (the project's first priority)

1. **QuickJS is the slowest link: JS does as little as possible.** `js/runtime` only pushes primitive values into one reused array. Parsing,
   diffing and string processing all happen in Rust. No JSON serialization on the JS side.
2. **Cross threads in batches.** One JS turn → one `__yf_commit` → one `Core::commit` → one `MountBatch` → one main-thread hop. Never cross FFI
   or threads per item.
3. **No needless allocation on hot paths.** Node ids are dense: index `Vec`s (`slots`), not hash maps. Internal links use slab keys. Reuse
   buffers (`take_ops`, `queue`).
4. **Do not send mount items that are not needed.** New views get no default style and no empty event mask; unchanged styles and frames are not
   sent. Check item counts with `yf trace`.
5. **Text**: measurements are cached per constraint (`TextContext.cache`); platforms cache font objects (`FontCache`).
6. **Changes to hot paths come with before / after `yf trace` timings and item counts.** Current baselines (Apple Silicon):
   - counter, headless (debug): first frame ~0.5ms (57 ops / 48 mount items), one tap ~0.15ms (12 ops / 8 items).
   - counter, Catalyst (release): first frame ~1.05–1.15ms (of which ~0.7ms is cold CoreText layout of 10 texts), one tap ~0.12ms.
   - Hot reload: ~120ms from saving a file to the device running the new version (file event latency and ~30ms of bundling included).
   - gallery (debug, Catalyst): first frame with 22 image + text rows, 598 ops / 205 nodes, ~3.7ms (list bind rounds included).
   - gallery scrolling (release): a frame that rebinds one row is 4 ops / 4 mount items; frames that reveal no new row call no JS and produce
     no batch. `list_bench` runs at ~20–30µs per frame back to back (JS ~20µs) and ~40µs with caches flushed between frames, but ~150µs after the
     thread sleeps 10ms. The difference is the CPU clocking down while the thread sleeps (Apple CLPC), not the code path; neither QoS (the engine
     thread is USER_INTERACTIVE) nor the Mach time-constraint policy changes it. Judge optimizations by the back-to-back and flushed-cache numbers:
     single-frame `yf trace` timings are dominated by the clock state.
   - On Catalyst (release), rebinding one row while scrolling slowly takes ~0.4ms (clocked down) and rebinding a whole screen of ~32 rows on a
     fast fling 0.9–1.6ms.
7. The dev profile builds `rquickjs-sys` and `taffy` with `opt-level = 3`, so debug timings are meaningful too; release uses fat LTO.

## Verified constraints and pitfalls

- **rquickjs must not enable the `full` feature.** It pulls in `dyn-load` (dlopen2), which does not compile for iOS / Catalyst. iOS targets
  have no prebuilt bindings and need the `bindgen` feature (and libclang).
- **QuickJS microtasks are run by the host.** Call `Host::run_jobs()` after every call from Rust into JS. `setTimeout` / `console` are our own;
  `queueMicrotask`, `WeakRef` and `FinalizationRegistry` are built into QuickJS-NG.
- **The JS object graph must be reachable from a global.** `js/runtime` keeps the root, the registry and `listening` on `globalThis.__yf`.
  Without it the whole rendered app (nodes, handlers, signals, registry) is an isolated cycle that idle GC collects, and taps stop working.
  `yf trace` taps after 1.2s of idling by default; `crates/yf-engine/tests/counter.rs` is the regression test.
- **Nodes are released by GC, not by Remove.** Solid may keep removed nodes in a memo and insert them again, so `Remove` only detaches;
  `Release` (from FinalizationRegistry) frees the node and its native view.
- **`yf_js::BUNDLE_NAME` (`bundle.js`)** is the module name of every bundle and is how stacks are matched for symbolication; `BYTECODE_VERSION`
  is tested against the linked QuickJS version: update both when upgrading rquickjs.
- **Use `oxc_sourcemap` 9.x** (matching rolldown / oxc 0.153); the oxc 0.118 inside the Solid compiler uses 6.x and only exchanges JSON strings,
  so never mix the types.
- **Our plugin's own compile errors** are kept in `YfPlugin.error` and shown directly (clean code frame + relative path) instead of rolldown's
  wrapped report.
- **UIFont and CTFont are toll-free bridged**: the text engine gets system fonts with `UIFont::systemFontOfSize_weight` and uses them as
  CTFonts (verified by rendering). CoreText attributes must use CT keys (colors are CGColors), not UIKit's NSForegroundColor.
- **Op buffer opcodes** are defined in both `js/runtime/index.ts` and `crates/yf-js/src/protocol.rs` and must stay in sync.
- **Text nodes outside `<text>`** (e.g. empty placeholders of `<Show>` / `<For>`) create no native view and take no part in layout. A `<text>`
  nested in `<text>` is a span without a native view; the whole block is merged into one `AttributedText`.
- **`TextSpan` offsets are UTF-16**, like NSAttributedString / Android Spannable.
- **Taffy 0.15 custom measure callbacks must delegate to `taffy::compute_leaf_layout`** and only compute the content size. Returning the
  content size directly skips resolving the leaf's own style (width / height / padding / min / max), so leaves such as
  `<image style={{width: 48}}>` end up with a zero size.
- **List cells are independent Taffy roots** (not children of the list's Taffy node; their width follows the list's cross axis): scrolling only
  relays out dirty cells and only emits frames for relaid cell subtrees; when the main tree is clean, `compute_layout` and the frame walk of the
  main tree are skipped (Taffy runs a full rounding pass even over a clean root).
- **Taffy defaults are CSS semantics**; `Style::default()` switches to RN semantics: column, flexShrink 0, alignContent flex-start, position relative.
- **The bundler platform must not be Node.** solid-js's `node` export condition points to the non-reactive SSR build.
- **`dom-expressions-compiler` is pinned to a commit on main.** Published tags predate the `compile` API we use. The project is young: run the
  full test suite before upgrading.
- **`MountItem::Insert` positions by sibling (`before`), not by index**: platforms insert their own subviews into containers
  (`UIRefreshControl` at the bottom, scroll indicators), which would shift indices.
- **Catalyst's Mac idiom does not support `UIRefreshControl`** (attaching one throws `_UICatalystUnsupportedMacIdiomBehavior`): in the Mac
  idiom the pull is detected in the scroll delegate and shown with a spinner + contentInset; iOS uses the native control.
- **Mac idiom custom pull to refresh**: on release, the system's bounce already targets the old inset (back to 0), so changing only
  `contentInset` hides the spinner; once refreshing starts, the offset is animated to `-REFRESH_INSET` on the next main-queue turn. Synthetic
  tests need continuous scroll events with phases (began / changed / ended); plain wheel events never overscroll.
- **`yf run` installs the host by copying to a temporary file and renaming it**: overwriting in place an executable that another session is
  running makes the kernel SIGKILL the newly launched process for a code signature mismatch.
- **Set backgrounds on `layer.backgroundColor`, not `UIView.backgroundColor`**: views with `drawRect:` (`TextView`) paint the latter into their
  contents bitmap, where corners cannot round it.
- **Styles arrive before frames** (new views have a zero size): a shared radius goes straight to the layer (Core Animation clamps it itself);
  only paths, shadow paths, the content view and the image layer depend on the size, and they are rebuilt on `Frame` while `Decor::sized()`
  is true. Scaling radii by the CSS rule at a zero size once turned every rounded corner square.
- **Property changes on standalone `CALayer`s animate implicitly for 0.25s** (a UIView's own layer does not): wrap changes to shape / image
  layers in `CATransaction.setDisableActions(true)`.
- **Core Animation always draws `border` above sublayers**, while CSS borders are below children: non-clipping views use a stroke layer below
  the children; when clipping both look the same, so the layer's own border is used.
- **A UIScrollView's sublayers scroll with its content** (`bounds.origin` is the scroll offset), so scroll views cannot use shape layers / masks.
- **UIKit callbacks may fire synchronously while the Mounter applies a batch** (e.g. setting `contentOffset` triggers `scrollViewDidScroll`):
  when `EventTarget::with_mounter` cannot borrow, it defers to the next main-queue turn; never `borrow_mut` directly.
- **Views with a transform must not set `frame`**: always use `bounds` + `center`, keeping the bounds origin (a scroll view's bounds origin is
  its scroll offset).
- **Catalyst**: `UIDeviceFamily` must include 6 (Mac idiom), otherwise the UI is scaled to 77%. Under Catalyst the container extends below the
  title bar, so overlays are constrained to `safeAreaLayoutGuide`. UIDevice reports "iPad" under Catalyst. Use the UIScene lifecycle
  (`UIScreen.mainScreen` and `UIWindow initWithFrame` are deprecated). The window size is fixed to 390×844 through `sizeRestrictions`.
- Methods in `define_class!` that return `Retained<T>` need `#[unsafe(method_id(...))]`. Classes UIKit instantiates by name (AppDelegate,
  SceneDelegate) cannot have ivars; keep their state in statics or thread locals.

## Verification after changes

1. `cargo test --workspace`, plus fmt and clippy as listed above (including the Catalyst target).
2. Changes to the JS runtime, engine or core: run `yf trace` and check item counts and timings.
3. Changes to the platform layer: launch with `yf run`, check screenshots and simulate input where needed (the terminal needs the Screen
   Recording and Accessibility permissions; while the screen is locked the frontmost app is `loginwindow` and clicks fail).
