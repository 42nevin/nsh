# Architecture

## Data flow and threads

```
JS (engine thread, QuickJS)  Solid components -> js/runtime pushes changes into one flat array -> __nsh_commit(ops) in a microtask
nsh-js                        decodes the op buffer in Rust (tags, style objects and event names are parsed here) -> Vec<Op>
nsh-core (engine thread)      apply(Op) maintains the shadow tree -> commit(): rebuild text, Taffy layout, diff frames -> MountBatch
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
- One `Surface` (Apple) / `NshView` (Android) is one engine (one QuickJS runtime), with no global singletons, so nsh can be embedded in
  existing apps with several instances. `AppView` (Apple) wraps a surface for embedding and owns its dev session; a standalone app is
  just "one AppView filling the window".
- **Android**: the engine thread typesets with `StaticLayout` through JNI; each commit is one `byte[]` plus an object
  table handed to `Mounter.java` on the UI thread, and animation items stay in Rust (the flow, the units and the JNI
  rules: `crates/nsh-platform-android/src/encode.rs`).
- **Networking**: `fetch` (and `Headers`, `Request`, `Response`, `AbortController`, `URL`, `URLSearchParams`, `TextEncoder`/`Decoder`)
  is defined by `crates/nsh-js/src/web.js`, precompiled to bytecode at build time and loaded on first use of any of those globals
  (zero startup cost for apps that do not use them). Requests go to the platform stack through `nsh_engine::Network`
  (NSURLSession, HttpURLConnection; ureq for `nsh trace`).

## Crates

Dependency direction: `nsh-core` ← `nsh-js` ← `nsh-engine` (also ← `nsh-package`) ← `nsh-platform-*` ← `nsh-ios-host` / `nsh-ios-app`; `nsh-cli` depends on `nsh-bundler`, `nsh-js` and `nsh-engine`. `nsh-core` must not depend on any platform or JS crate.

The CoreText and StaticLayout text engines are documented in `crates/nsh-platform-ios/src/text.rs` and
`crates/nsh-platform-android/src/text.rs`, the measure cache in `crates/nsh-core/src/text.rs` (`MeasureCache`).
