# AGENTS.md

yf is a React Native–like cross-platform UI framework: app code is written in TSX + SolidJS and runs in QuickJS; layout uses Taffy; everything renders with the system's native views (UIKit / Android View), so embedding native components such as maps or web views has no layering problems. The toolchain is pure Rust and does not need Node.js; app developers get a prebuilt distribution (`cargo xtask dist`) and never need Rust either.

## Layout

```
crates/
  yf-core/            Platform-independent core: shadow tree, styles (RN semantics), Taffy layout (flex + grid), text merging, list virtualization,
                      mount items; CSS values (`css.rs`: colors, lengths, shadows, gradients, filters), effects (`effects.rs`), Tailwind (`tailwind/`)
  yf-anim/            Native animation graph (pure Rust): value / interpolate / math nodes, timing / spring / decay drivers
  yf-js/              QuickJS host: console / timers / op bridge, op buffer decoding, bytecode compilation and loading
  yf-engine/          Engine thread: drives yf-js + yf-core and hands one MountBatch per frame to the platform
  yf-bundler/         Bundler: rolldown + Oxc (TS stripping) + Solid universal JSX compiler + embedded JS packages + source maps;
                      the `yf:tailwind-theme` module (yf.json `tailwind`) and the unknown-class check (`classes.rs`);
                      development bundles with hot module replacement (`hot.rs`); the TypeScript declarations of `.yf/types` (`types.rs`)
  yf-dev/             Dev tooling: dev server protocol, server (CLI, feature `server`), client (host, feature `client`)
  yf-platform-apple/  UIKit backend (iOS / simulator / Mac Catalyst): native view mounting, CoreText text engine, input, AppView, standalone app;
                      feature `dev`: dev server client, log forwarding, error overlay
  yf-platform-android/ Android backend: `libyf.so` (JNI glue, StaticLayout text engine, batch encoding, UI-thread animator) + the Java
                      runtime in `java/dev/yf` (YfView, Mounter, views, images, networking); feature `dev` as above
  yf-host/            Development host app (Catalyst / iOS simulator); connects to a dev server or runs a local bundle
  yf-app/             Release runner: the executable of iOS apps built by `yf package`
  yf-ffi/             C ABI (`include/yf.h`, `libyf.a`) for embedding yf in existing iOS apps; shipped as an XCFramework
  yf-cli/             The `yf` command line: init / build / trace / dev / run / package
  xtask/              Maintainer tasks: `cargo xtask android` (Android runtime), `cargo xtask dist` (the distribution)
js/runtime/index.ts   Solid universal renderer (JS side) and the runtime components (List, Stack, Modal, DrawerLayout, ...), bundled by yf-bundler
js/runtime/types.ts   Style, element-prop and JSX types (types only; also the source of `.yf/types/yf/types.d.ts`)
js/runtime/globals.d.ts  Declarations of the JS globals (console, timers, fetch, ...) and of asset imports
js/runtime/hot.js     Hot module replacement runtime of development bundles (module registry, component proxies)
js/vendor/solid-js/   Embedded solid-js (version in VERSION) and its type declarations (`types/`, `jsx.d.ts` replaced by yf's)
examples/             Example apps (gallery: a demo index, one module per demo; counter) and embedding samples (embed-ios, embed-android)
```

Dependency direction: `yf-core` ← `yf-js` ← `yf-engine` ← `yf-platform-*` ← `yf-host` / `yf-app` / `yf-ffi`; `yf-cli` depends on `yf-bundler`, `yf-js` and `yf-engine`. `yf-core` must not depend on any platform or JS crate.

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
- One `Surface` (Apple) / `YfView` (Android) is one engine (one QuickJS runtime), with no global singletons, so yf can be embedded in
  existing apps with several instances. `AppView` (Apple) wraps a surface for embedding and owns its dev session; a standalone app is
  just "one AppView filling the window".
- **Android**: the engine thread typesets with `StaticLayout` through cached JNI method ids (classes are cached in `JNI_OnLoad`:
  native threads only see the system class loader). Each commit is encoded (`encode.rs`) into one `byte[]` plus an object table
  (strings, layouts) and handed to `YfView.submit` → `Mounter.java` on the UI thread. Animation items stay in Rust: Java calls
  `Native.afterBatch(seq)` after applying a batch, and `Choreographer` frames call `Native.tick`; both return
  `[animating, (view, prop, value)*]` float arrays. Units cross as dp; Java converts to px.
- **Networking**: `fetch` (and `Headers`, `Request`, `Response`, `AbortController`, `URL`, `URLSearchParams`, `TextEncoder`/`Decoder`)
  is defined by `crates/yf-js/src/web.js`, precompiled to bytecode at build time and loaded on first use of any of those globals
  (zero startup cost for apps that do not use them). Requests go to the platform stack through `yf_engine::Network`
  (NSURLSession, HttpURLConnection; ureq for `yf trace`).

## Commands

```sh
cargo build -p yf-cli                                         # build the yf CLI
./target/debug/yf init my-app                                 # new app: App.tsx, yf.json, tsconfig.json, .yf/types
cargo build -p yf-host --target aarch64-apple-ios-macabi      # build the Catalyst development host (needed by yf run)
cargo build -p yf-host --target aarch64-apple-ios-sim         # the iOS simulator host (`yf run --target ios`)
cargo build -p yf-app --target aarch64-apple-ios-sim          # the release runner (`yf package --target ios-simulator`)
./target/debug/yf trace examples/counter/App.tsx              # headless run: prints mount items and the time of every commit
./target/release/yf trace examples/gallery/Bench.tsx --scroll 3000 --taps 0   # headless scrolling (10pt per frame): per-frame list cost
./target/debug/yf run examples/counter/App.tsx --trace        # dev server + Catalyst host (390×844), hot reload on save
./target/debug/yf run examples/gallery/App.tsx --target ios   # in an iOS simulator (boots one; needs the yf-host sim build)
./target/debug/yf run examples/gallery/App.tsx --target android   # on an emulator / device (needs `cargo xtask android --dev`)
./target/debug/yf package examples/gallery/App.tsx --target ios-simulator --run   # installable app (.app / .ipa / .apk)
cargo xtask android [--release] [--dev]                       # Android runtime into target/android/<profile>[-dev]
cargo xtask dist                                              # the distribution in dist/yf
./target/debug/yf dev examples/counter/App.tsx                # dev server only (for simulators / devices); prints local and LAN URLs
./target/debug/yf build examples/counter/App.tsx --release    # writes bundle.js / bundle.qbc

cargo test --workspace
cargo test --release -p yf-engine --test list_bench -- --ignored --nocapture   # gallery scroll benchmark (JS + core)
cargo test --release -p yf-core bench_ -- --ignored --nocapture                # row / cell relayout benchmarks
cargo +nightly fmt --all                                      # .rustfmt.toml uses nightly options
cargo clippy --workspace --all-targets
cargo clippy -p yf-platform-apple -p yf-host --target aarch64-apple-ios-macabi
```

`yf-platform-apple` only compiles for `target_os = "ios"` (iOS devices, the iOS simulator and Catalyst); on a macOS host it is an empty crate, so always run clippy on platform code with the Catalyst target. Likewise `yf-platform-android` is empty off Android: check it with `cargo xtask android` (which sets up the NDK linker, `CC`, `AR` and bindgen's sysroot) and `cargo xtask clippy-android`.

The CLI finds prebuilt runtimes next to itself (`bin/../runtime`, the distribution layout) or, in a checkout, in
`target/<triple>/<profile>` and `target/android/<profile>[-dev]` with the CLI's own profile (`YF_RUNTIME` overrides).
`.cargo/config.toml` sets `IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.

## Elements and capabilities

| JSX | Native (Apple) | Notes |
|---|---|---|
| `<view>` | `YFView` (UIView) | `onPress` / `onLongPress` / `onPressIn` / `onPressOut`, `activeOpacity`, `pan={{x, y}}` + `onPanStart` / `onPanEnd`, `pinch` / `rotation` |
| `<text>` | `TextView` (CTFrame) | nested `<text>` is a span; `numberOfLines`, `ellipsizeMode`, `allowFontScaling`, `maxFontSizeMultiplier` |
| `<image src>` | UIView + image CALayer | `src` is a URL or `import img from "./a.png"` (an asset descriptor with its size); `resizeMode`; `defaultSource` (placeholder), `blurRadius`, `fadeDuration`; `onLoadStart` / `onLoad` (the source's pixel size) / `onError` / `onLoadEnd`; `tintColor`; animated GIF / WebP / APNG |
| `<scroll>` | UIScrollView | `horizontal`, `refreshing` + `onRefresh`, `scrollValue`, `onScrollEnd`, `keyboardDismissMode`, see **Scrolling** below |
| `List`, `SectionList` (runtime) | UIScrollView + recycled cells | see **List** below |
| `<input>` | UITextField / UITextView in a container | `value` / `defaultValue` / `onChangeText` / `onSubmitEditing` / `onFocus` / `onBlur`, `placeholder`, `secureTextEntry`, `keyboardType`, `returnKeyType`, `autoCapitalize`, `autoCorrect`, `multiline`, `maxLength`, `editable`, `autoFocus`, `selectionColor`, `blurOnSubmit`, `selection` + `onSelectionChange` (UTF-16 `start` / `end`), `onKeyPress` (`key`: the text, `Backspace`, `Enter`), `autoComplete` / `textContentType` (autofill); ref `setSelection(start, end)` |
| `Stack` / `Screen` (runtime) | UINavigationController | `createNavigator(route)`: `push` / `pop` / `popToTop` / `replace` / `reset`; screens keep their state while stacked; native header (`title`, `headerLeft` / `headerRight` / `headerTitle` elements, `headerLargeTitle`, `headerTransparent`, colors, `headerBackTitle`), swipe back |
| `Modal` (runtime) | presented UIViewController | `visible`, `animationType`, `presentationStyle` (`pageSheet` / `formSheet` on iOS), `transparent`, `statusBarTranslucent` (Android; otherwise the content starts below the status bar), `supportedOrientations` (iOS), `onRequestClose` (controlled), `onShow`, `onDismiss` |
| `InputAccessoryView` (runtime, iOS) | `<inputaccessory>`: a YFView set as `inputAccessoryView` | `nativeID`; inputs name it with `inputAccessoryViewID`. A layout root as wide as the window and as tall as its content; the core resolves the id (`InputConfig::accessory`). Renders nothing on Android (as React Native) |
| `WebView` | WKWebView (WebKit loaded on first use) | `source` (`uri` / `html` + `baseUrl`), `injectedJavaScript(BeforeContentLoaded)`, `onMessage` (pages call `window.ReactNativeWebView.postMessage`), `onLoad*`, `onNavigationStateChange`; ref: `reload()`, `goBack()`, `goForward()`, `postMessage(data)`, `injectJavaScript(js)` |
| `Video` | AVPlayerLayer / AVPlayerViewController (AVFoundation loaded on first use) | `source`, `paused`, `muted`, `volume`, `rate`, `repeat`, `resizeMode`, `controls`, `onLoad` / `onProgress` / `onEnd` / `onBuffer` / `onError`; ref: `seek(seconds)` |

On Android the same elements map to `Box` (ViewGroup), `TextBox`, `ImageBox`, `ScrollBox` (ScrollView), `InputBox` (EditText),
`StackBox` (screens with a Toolbar, predictive back), `ModalHost` (a Dialog), `WebBox` (android.webkit.WebView) and `VideoBox`
(MediaPlayer on a TextureView, MediaController); both are `WidgetBox`es. Every host element and runtime component is typed in
`js/runtime/types.ts` / `index.ts`.

**No platform-styled widgets.** yf provides primitives; apps (and the UI library built on yf) draw controls such as switches,
sliders, spinners and buttons with views, classes and native animations, so they look the same on iOS and Android (the gallery's
`Toggle` in `examples/gallery/ui.tsx`). Do not add wrappers of UISwitch / SeekBar and the like.

- **Runtime components on the primitives**: `TouchableOpacity` (`activeOpacity` 0.2), `TouchableHighlight` (`underlayColor` +
  `activeOpacity` for the children: `underlayColor` / `underlayOpacity` on any view, applied by the core while the platform
  reports the view pressed — no JS round trip), `TouchableWithoutFeedback` (spreads its props on its only child),
  `ImageBackground`, `DrawerLayout` (alias `DrawerLayoutAndroid`; both platforms: one native value is the offset, the panel, the
  scrim and a 20pt edge strip drive it with `pan`, `onPanEnd`'s `valueX` / velocity decide where it settles; ref:
  `openDrawer()`, `closeDrawer()`, `progress` (a native node 0..1)), `KeyboardAvoidingView` (`behavior`: `padding` →
  `insetPadding`, `height` → `insetMargin`, `position` → an inner view moved up; `keyboardVerticalOffset`).
- **Platform modules**: `Platform` (`OS`, `Version`, `isPad`, `isTV`, `isMacCatalyst`, `select`), `PixelRatio`, `StyleSheet`
  (`create`, `flatten`, `compose`, `hairlineWidth`, `absoluteFill`), `AccessibilityInfo` (`isScreenReaderEnabled`,
  `isReduceMotionEnabled`, `addEventListener("screenReaderChanged" | "reduceMotionChanged")`, `announceForAccessibility`,
  `setAccessibilityFocus(ref)`), reactive `screenReaderEnabled()` / `reduceMotion()`. The environment carries `pixelRatio`,
  `screenReader`, `osVersion` and `idiom` (VoiceOver / TalkBack changes are observed natively).
- **`disabled`** on any element applies `disabled:` classes and removes its press listeners from what is mounted (no press,
  no feedback).
- **`onPanEnd` / `onPanStart`** carry `valueX` / `valueY`: the bound pan values at that moment (a drag may grab a value mid
  animation), for snapping.

- **Text**: `<text numberOfLines ellipsizeMode="tail|head|middle|clip">`; styles `fontFamily` (system or imported fonts:
  `import Inter from "./Inter.ttf"` evaluates to the family name and registers the font before the first frame), `fontWeight`,
  `fontStyle`, `letterSpacing`, `textDecorationLine` + `textDecorationColor`, `textTransform`, `lineHeight`, `textAlign`.
- **Imperative calls**: nodes have `focus()`, `blur()` and `scrollTo({ x, y, animated })` (use `ref`); lists also
  `scrollToIndex({ index, viewPosition, viewOffset, animated })`, `scrollToEnd()`, `scrollToOffset()`; `Keyboard.dismiss()`.
  Commands run after the commit's other mount items; list scrolls are resolved by the core with the current item sizes.
- **Touch** (React Native semantics): `pointerEvents`, `hitSlop`, `activeOpacity`, `delayLongPress`, `android_ripple`
  (`{ color, borderless, radius, foreground }`, Android only); press-in waits ~130ms inside scroll views; only the innermost
  pressable view responds. `Pressable` passes `{ pressed }` to `style` / `children`. Nested `<text>` spans with press handlers
  are links: the core lists them in `AttributedText::links` (UTF-16 ranges) and the block's press recognizer routes a press to
  the span under the finger (CoreText `CTLineGetStringIndexForPosition` / `Layout.getOffsetForHorizontal`).
- **Raw touches and the responder system**: `onTouchStart/Move/End/Cancel` (+ `Capture`), `onStartShouldSetResponder`,
  `onMoveShouldSetResponder` (+ `Capture`), `onResponderGrant/Reject/Start/Move/End/Release/Terminate/TerminationRequest`, and
  `PanResponder.create` on top. A node with any of them listens to `onTouch` natively; one tracker per root view (surface,
  modals: `touch.rs`, `TouchTracker.java`) observes without recognizing and reports touches whose target or an ancestor
  listens, with the path of listening nodes (`EventPayload::Touch`, sent to the root; moves are merged per engine turn). The
  negotiation runs in JS as in React Native. A grant returning true (PanResponder's default) sends `setJsResponder`: scroll
  views around stop taking the touches (UIKit: their pan recognizer is disabled; Android: `ScrollBox.jsBlocked`); a native
  scroll view that starts dragging first cancels the JS touches.
- **Accessibility**: `accessible`, `accessibilityLabel` / `Hint` / `Role` (`role`) / `State` / `Value` / `Actions` +
  `onAccessibilityAction`, `accessibilityElementsHidden`, `importantForAccessibility`, `accessibilityLiveRegion`, `testID`
  (Android: resource id). Views without a label read their texts.
- **Layout animation**: `LayoutAnimation.configureNext(config)` (or the `easeInEaseOut` / `linear` / `spring` presets) animates
  the next commit's frames, insertions and removals natively.
- **Frames**: `requestAnimationFrame` / `cancelAnimationFrame` follow the display (CADisplayLink / Choreographer).
- **Back button** (Android): `BackHandler.addEventListener("hardwareBackPress", ...)`, after modals and stacks; the app exits
  when nothing handles it.
- **Scrolling**: `pagingEnabled`, `snapToInterval` / `snapToOffsets` / `snapToAlignment`, `disableIntervalMomentum`,
  `decelerationRate`, `scrollEnabled`, `bounces`, `contentInset`, `contentOffset`, `keyboardShouldPersistTaps`
  (`never` / `handled` / `always`), `onScroll` (+ `scrollEventThrottle`; at most once per engine turn), `onScrollBeginDrag` /
  `EndDrag`, `onMomentumScrollBegin` / `End` (a glide caught by a new drag ends with `onMomentumScrollEnd`). Snapping is decided
  by `ScrollBehavior::snap_target` (core) on iOS and its Java port on Android, from the release offset, the projected glide and
  the velocity. Lists keep the visible rows in place when items are inserted above them (`splice`).
- **System UI**: `StatusBar` (component and static methods; the last mounted wins, prop by prop), `Alert.alert`,
  `ActionSheetIOS.showActionSheetWithOptions` (a list dialog on Android), `ToastAndroid`. Answers come back as the root's
  `onDialog` event. Embedding apps forward their controller's status bar to `yf_status_bar_style()` / `_hidden()` / `_animation()`.
- **Right to left**: `I18nManager` (`isRTL`, reactive; `allowRTL`, `forceRTL`, applied at once) sets the root's direction;
  `direction` (`inherit` / `ltr` / `rtl`) overrides it per view. The core resolves each node's direction (inherited, also for
  subtrees built detached), sets taffy's `direction` (rows run from the right) and, right to left, swaps the left and right
  properties (margins, paddings, insets, borders, corners — React Native's default `doLeftAndRightSwapInRTL`) and places the
  start / end ones (`marginStart`, `start`, `borderTopEndRadius`, ...). A direction change swaps the current taffy style in place
  (an involution), so authored styles need not be kept. Text: `auto` alignment is the right side right to left, left / right
  swap, `writingDirection` sets the base direction (else the content's). Native widgets mirror (`MountItem::LayoutDirection`:
  `semanticContentAttribute`, `View.setLayoutDirection`; Android apps need `android:supportsRtl`). Horizontal scroll views
  are laid out left to right and their children mirrored in the content by the core (a right-to-left row would overflow to
  negative offsets), starting at the right end and keeping their distance from it; horizontal lists are flipped natively
  (`ScrollConfig::mirrored`: the scroll view and each cell get scaleX −1, composed with their transforms), so the list's
  offset logic stays left to right.
- **Environment** (reactive accessors from `"yf"`): `windowSize()`, `safeArea()`, `keyboard()` (`{ height, duration }`: how
  far the keyboard reaches into the surface, the bottom safe area under it included, on every platform),
  `colorScheme()`, `fontScale()`. `KeyboardAvoidingView` pads its bottom by `keyboard().height`, animated with the keyboard,
  for content outside scroll views (message bars). Inputs inside a scroll view are kept clear of the keyboard natively (bottom inset + scroll to the
  focused input), with no JS involved.
- **Controlled inputs**: each native edit carries an edit count; a `value` from JS is applied only if it is based on the latest
  edit, so a slow JS round trip can never overwrite newer typing (React Native's eventCount).

- **Tailwind** (`class` / `classList` on every element; `tailwind/`, `core/classes.rs`): JS only passes the strings. Each distinct
  class string is parsed once into rules (an LRU of ≤4096 strings) with what they depend on (`deps`: appearance, window width /
  height, direction, the node's own states, a group's states, sibling position, reduce motion). A node's style is the default,
  then its matching class rules, then its inline `style` replayed on top (`Op::SetStyle` carries a `StylePatch` besides the
  merged `Style`), so the inline style always wins. Later utilities win over earlier ones (components append a `class` prop, no
  `tailwind-merge`); variant rules apply after plain ones in Tailwind's variant order. Variants: `dark:`, `sm:`…`2xl:` / `max-*:` /
  `min-[..]:`, `portrait:` / `landscape:`, `ios:` / `android:`, `rtl:` / `ltr:`, `motion-safe:` / `motion-reduce:`, `active:` /
  `hover:` / `focus:` / `focus-within:` / `disabled:` / `enabled:`, `group-*:` (named groups `group/name`), `first:` / `last:` /
  `odd:` / `even:` / `only:`, `placeholder:`, `!` (important). Interaction states are reported natively only for the bits a node's
  classes read (`MountItem::TrackStates` → `EngineControl::set_states`); `focus-within` propagates to ancestors in the core.
  `disabled` (any element) applies `disabled:` and drops the node's press listeners from what is mounted, so it shows no press
  feedback either. The theme is Tailwind 4's (palette converted from OKLCH to sRGB) extended by `yf.json` `"tailwind"` (`colors`,
  `spacing`, `breakpoints`, `fontSize`, `radius`, `shadow`, ...; unknown keys are build errors), compiled into the bundle as
  `yf:tailwind-theme` and pushed as a root prop at startup. `yf build` / `yf dev` warn about unknown static classes
  (`file:line:col: unknown class '...'`, target `yf::build`); the core logs each unknown class once at run time. Class-only
  settings that are props in React Native (`line-clamp-*`, `truncate`, `whitespace-nowrap`, `pointer-events-*`, `object-*`,
  `placeholder:text-*`, `caret-*`) lose to explicit props. Not supported: `peer-*`, pseudo-elements, arbitrary CSS properties;
  `space-x/y-*` are gaps.
- **Text style inheritance** (CSS-like): a view's text properties (color, font, size, alignment, ...) pass to the texts and inputs
  inside it, so `class="text-white"` on a button styles its label. `insetPadding` (runtime only: `SafeAreaView`,
  `KeyboardAvoidingView`) is added to the resolved padding, so classes and styles keep their own padding.
- **Effects** (styles and classes; `effects.rs`): `boxShadow` (CSS syntax, several, `inset`; also `shadow-*` / `ring-*`),
  `experimental_backgroundImage` / `backgroundImage` (linear / radial / conic gradients), `filter` (`brightness`, `contrast`,
  `grayscale`, `hue-rotate`, `invert`, `saturate`, `sepia`, `opacity`, `blur`, `drop-shadow`; color filters are composed into one
  matrix), `backdropFilter` (blur), `mixBlendMode`, `outline*`, `visibility`, `cursor`, `transition*` (CSS: property groups,
  duration, delay, easing) and `animation*` (Tailwind's `spin` / `ping` / `pulse` / `bounce`), `display: grid` with
  `gridTemplateColumns/Rows`, `gridAutoFlow/Columns/Rows`, `gridColumn/Row(Start/End)`, `justifyItems/Self`; text
  `textDecorationStyle` (`double` / `dotted` / `dashed` / `wavy`), `textDecorationThickness`, `textUnderlineOffset`, `em`
  letter spacing and unitless line heights (multiples). Translations by a fraction of the size (`translate-x-1/2`) are resolved by
  the core against the frame and sent again when the size changes.
  iOS: shadows are one `CALayer` per shadow with a `shadowPath` (outer shadows masked to outside the box, inset ones to the padding
  box), gradients `CAGradientLayer`, transitions explicit `CABasicAnimation`s from the presented values, keyframes
  `CAKeyframeAnimation`s, blend modes `compositingFilter`, `cursor` a `UIPointerInteraction`, backdrop blur a
  `UIVisualEffectView` (a system material at a fraction: an approximation), text decorations other than solid single lines are
  drawn by `TextView`. Android: `Decor` draws shadows, gradients and outlines; filters are `RenderEffect`s (API 31+; below, only
  color filters through a layer paint), blend modes a layer paint (API 29+), transitions / keyframes `ValueAnimator`s.
  Known limits: iOS has no CSS filters on views (only `opacity`, `brightness` ≤ 1 and `drop-shadow`; images take every color filter
  and `blur`, applied while decoding); iOS texts draw no gradients or inset shadows; Android has no backdrop blur, and no `blur` /
  `drop-shadow` before API 31.
- **View styles**: `backgroundColor`, `borderWidth` / `borderColor` (also per side: `borderTopWidth`, `borderLeftColor`, ...),
  `borderStyle` (`solid` / `dashed` / `dotted`), `borderRadius` + the four `borderTopLeftRadius`-style corners, `overflow`, `opacity`,
  `transform` (translate / scale / rotate / rotateX / rotateY / skew / perspective, applied right to left like CSS; Android has no
  skew), `zIndex` (stable order of the native views), `shadowColor` / `shadowOffset` / `shadowOpacity` / `shadowRadius` (RN
  defaults: offset `(0, -3)`, radius 3, opacity 0), `tintColor` (images), `elevation` (Android's material shadow, with
  `shadowColor` from API 28), `transformOrigin` (keywords / percentages / lengths; iOS conjugates the matrix around it,
  Android sets the pivot), `backfaceVisibility` (iOS `doubleSided`; Android hides the view while its rotations turn it away). `style` may be an array of styles merged left to right
  (falsy entries skipped), decoded in Rust. Text adds `textShadowColor` / `textShadowOffset` / `textShadowRadius`,
  `fontVariant` (OpenType `smcp` / `onum` / `lnum` / `tnum` / `pnum`: CoreText feature settings, `Paint.setFontFeatureSettings`)
  and `writingDirection`.
  On Apple everything is drawn by Core Animation (`decor.rs`), escalating only as needed: rounded corners sharing one radius (possibly only some
  corners) → the layer's own `cornerRadius` + `maskedCorners`; different radii → `CAShapeLayer` fill / stroke / mask; clipping + shadow → the
  children move into a content view (clipping a layer clips its shadow too). With a background the shadow gets a `shadowPath` (no offscreen
  pass); with a transparent background the shadow follows the content (e.g. text shadows).
  Known limits: scroll / list only support one radius (the largest wins) and clip their shadow; `<text>` with different radii is masked, which
  clips its shadow too.
- **List** props: `each`, `estimatedItemSize`, `numColumns`, `inverted` (chat: item 0 at the bottom, starts at the end and keeps its
  distance from the end as content changes), `stickyHeaderIndices` (one column, not inverted), `ListHeaderComponent` /
  `ListFooterComponent` / `ListEmptyComponent` / `ItemSeparatorComponent` (a component or an element), `contentContainerStyle`
  (padding and gaps, numbers), `initialScrollIndex`, `onEndReached` + `onEndReachedThreshold`, `onViewableItemsChanged` +
  `viewabilityConfig` (`itemVisiblePercentThreshold` / `viewAreaCoveragePercentThreshold` / `minimumViewTime`), `keyExtractor`,
  `refreshing` / `onRefresh`, `scrollValue`. `SectionList` (`sections`, `renderSectionHeader`, `renderItem`) is a `List` of header
  and item rows with sticky headers. Positions, columns, sticky travel and viewability are all computed in the core; sticky cells
  are pinned natively on every scroll (`MountItem::StickyCells`: translate by `clamp(offset - start, 0, end - start)`).
- **List internals**: the window is computed in Rust (`core/list.rs`, Fenwick prefix sums over rows), rendering one extra screen above and below the viewport.
  Rust asks JS to bind cells with `__yf_list(listId, [slot, index, …])`; JS updates the cell content inside `batch` and then sets `cellIndex` to
  confirm, and core only measures / positions confirmed cells. New cells are created with their first index. The engine runs at most 8
  "bind → JS → layout" rounds per frame, merged into one batch. Row-local state travels with recycled cells: keep it outside, keyed by id.
  Unbound cells keep their last content, so the core moves them out of sight; the header, footer and empty placeholder are cells with
  a role (`slot="header"`, ...) that are never recycled.
- **Animation**: `createNativeValue()` → `timing` / `spring` / `decay` (return a Promise), `interpolate` (numbers, colors or
  `"deg"` / `"rad"` angles; `extrapolate` / `extrapolateLeft` / `extrapolateRight`: `clamp` (default) / `extend` / `identity`;
  `easing`), `add/sub/mul/div/mod`, `diffClamp(min, max)`. `Easing` (React Native's: `linear`, `ease`, `quad`, `cubic`, `poly`,
  `sin`, `circle`, `exp`, `elastic`, `back`, `bounce`, `bezier`, `in` / `out` / `inOut`) is data decoded in Rust
  (`yf_anim::Easing::Curve`), never a JS function. `Animated.timing` / `spring` / `decay` / `sequence` / `parallel` / `stagger` /
  `delay` / `loop` build a `Composite` tree that runs entirely natively (`AnimOp::StartComposite`): each step starts at the exact end
  time of the previous one (no frame gaps), `stop()` / `reset()` (back to the values at start) / `start(cb)` report `finished` once
  for the whole tree; leaf animations get internal ids (`LEAF_IDS`) that never reach JS. Composites are released by
  FinalizationRegistry (`ReleaseComposite`).
  Bindable: `opacity`, `backgroundColor`, and the translate / scale / rotate parts of `transform`. Driven on the main thread by `CADisplayLink`,
  paused while nothing animates. `scrollValue` writes the scroll offset into a native value in the same frame; `pan` lets drags drive native values
  directly, and JS receives the velocity when the drag ends. Native values referenced by a view's style are kept on the view node (`YNode.anims`)
  and derived nodes hold their inputs, so bindings do not break when JS objects are collected.
- **Images**: background NSURLSession / file read → ImageIO downsampling decode at the view's pixel size → `NSCache` (purged under memory pressure);
  requests for the same key are coalesced and downloads nobody waits for are cancelled (Android: HttpURLConnection / APK assets →
  `BitmapFactory` power-of-two sampling → `LruCache`, same coalescing). During development `asset:/path` goes through the dev
  server's `/assets/`; `yf build` copies assets to `out/assets/`, `yf package` puts them in the app. Cache keys include the decode
  size and the blur radius. `blurRadius` is a stack blur on the decoding thread (`blur.rs`, `Blur.java`: the same algorithm, in
  pixels). `defaultSource` loads after the image request and shows until the image arrives; the image then fades in over
  `fadeDuration` (iOS: `CATransition`; Android: alpha over the placeholder; default 0 on iOS and 300ms on Android, as in React
  Native). Animated images: iOS decodes up to 600 frames into an animated `UIImage` (frames repeated by their delays' common
  divisor) played by a discrete `CAKeyframeAnimation` on `contents` (a tinted animated image shows its first frame); Android
  (API 28+) uses `ImageDecoder` → `AnimatedImageDrawable`, not cached (each view plays its own copy; no blur). `onLoad` reports the
  source's size in pixels on both platforms, whatever size the view decodes at. Android `ImageBox` draws its borders above the
  image (as Core Animation's layer border).
- **Android differences**: shadows need a background (no content shadows, e.g. text with a transparent background); odd corner
  radii clip by path before API 33; pull to refresh is our own (stretch overscroll stays native); `contentInset` is padding and
  `decelerationRate` only shapes snapping glides.

## Packaging and embedding

- `yf package <entry> --target ios|ios-simulator|android` builds an installable app from the prebuilt runtime: metadata from
  `yf.json` next to the entry (`name`, `id`, `version`, `build`, `icon`) or flags. iOS: `.app`, `.ipa` with `--sign <identity>
  --profile <file.mobileprovision>` (entitlements come from the profile; icons are compiled by `actool` into `Assets.car`).
  Android: an APK assembled without Gradle (`aapt2 link`, native libraries stored uncompressed and 16 KB aligned, `apksigner`;
  the debug key unless `--keystore`).
- Embedding: iOS apps link `YF.xcframework` and use the C API in `crates/yf-ffi/include/yf.h` (`import YF` in Swift); Android apps
  add `yf.aar` and use `dev.yf.YfView`. `*Dev` / `-dev` variants can also load from a dev server. See `examples/embed-*`.

## TypeScript

- Every command that builds an app writes `.yf/types` next to its entry (only changed files): `yf/index.d.ts` and `yf/types.d.ts`
  generated from the runtime sources with Oxc isolated declarations, `yf/jsx-runtime.d.ts`, `yf/globals.d.ts`, and solid-js's
  declarations. `yf init` writes a `tsconfig.json` mapping `yf`, `yf/jsx-runtime` and `solid-js` there (`jsxImportSource: "yf"`,
  no DOM library), so editors type-check apps without `node_modules`.
- Exports of `js/runtime/index.ts` and `types.ts` need explicit types (isolated declarations cannot infer); a yf-bundler test
  enforces it. Keep `types.ts` in sync with what `style.rs` and `protocol.rs` accept.
- To check the declarations themselves, run `tsc -p .` in an app with `skipLibCheck` off (only solid's `hydration.d.ts`
  mentions the DOM `Element`).

## Dev server (yf dev / yf run)

- The CLI watches sources with notify and pushes every rebuild over WebSocket (`/ws`, default port 8790, the next free port if taken) to all
  connected hosts; new clients subscribe before receiving the current state, so no event is missed. Protocol: `crates/yf-dev/src/protocol.rs`
  (JSON text frames + binary frames).
- Bytecode version negotiation: the client reports `BYTECODE_VERSION` in its handshake; on a mismatch the server sends JS source instead.
- The host's `log` output (including JS `console.*`, target `js`) is forwarded to the CLI terminal with the device name while connected, and goes
  to stderr when disconnected.
- Build failures, bundle evaluation failures and uncaught exceptions show a red overlay on the host (tap to dismiss); it disappears on the next
  successful reload.
- **Hot module replacement** (`yf dev` / `yf run` without `--release`): development bundles keep one CommonJS chunk per module
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
   - counter, headless (debug): first frame ~0.5–0.65ms (58 ops / 49 mount items), one tap ~0.15ms (12 ops / 8 items).
   - counter, Catalyst (release): first frame ~1.05–1.15ms (of which ~0.7ms is cold CoreText layout of 10 texts), one tap ~0.12ms.
   - Hot reload: ~120ms from saving a file to the device running the new version (file event latency and ~60ms of development
     bundling included); applying a hot update takes 0.2–0.5ms on the engine thread.
   - gallery Photos list (`examples/gallery/Bench.tsx`, debug, headless): first frame 653 ops / 224 nodes, ~2.7ms (list bind
     rounds included).
   - Photos scrolling (release): a frame that rebinds one row is 4 ops / 4 mount items; frames that reveal no new row call no JS and produce
     no batch. `list_bench` runs at ~20–30µs per frame back to back (JS ~20µs) and ~40µs with caches flushed between frames, but ~150µs after the
     thread sleeps 10ms. The difference is the CPU clocking down while the thread sleeps (Apple CLPC), not the code path; neither QoS (the engine
     thread is USER_INTERACTIVE) nor the Mach time-constraint policy changes it. Judge optimizations by the back-to-back and flushed-cache numbers:
     single-frame `yf trace` timings are dominated by the clock state.
   - On Catalyst (release), rebinding one row while scrolling slowly takes ~0.4ms (clocked down) and rebinding a whole screen of ~32 rows on a
     fast fling 0.9–1.6ms.
   - Host startup (`Host::new`, release): ~155µs. The web APIs add nothing until first used (they used to cost ~0.8ms as source,
     ~0.33ms as bytecode).
   - Android emulator (arm64, release): rebinding one gallery row while scrolling ~0.3–0.4ms (JS ~60µs); text costs per new
     text: `PrecomputedText` ~120µs, measure ~50µs, final layout reused. First gallery frame 20–50ms (noisy on the emulator).
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
- **Taffy defaults are CSS semantics**; `Style::default()` switches to RN semantics: column, flexShrink 0, alignContent flex-start, position relative,
  and min sizes 0 (an unset or `auto` `minWidth` / `minHeight` is 0). Taffy's `auto` minimum is CSS's automatic minimum size (the
  min-content size); text has no cheap min-content and reports its natural width, so `flex: 1` text in a row could not shrink and
  overflowed instead of wrapping. Yoga has no automatic minimum.
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

- **UIKit implements the `UITextInputTraits` setters of `UITextField` by message forwarding**: objc2's debug-build signature
  check cannot find them on the class and panics, so `input.rs` sends them with a raw `objc_msgSend`.
- **objc2 crates are used with `default-features = false`** and an explicit feature list in the root `Cargo.toml` (one feature per
  header): the defaults link CloudKit, CoreData, CoreImage, CoreLocation, Metal, ... into every app (each one costs dyld time at
  launch). When a method is missing after using a new API, enable the header feature named in its `#[cfg(feature = ...)]`; check
  `otool -L` on a release `yf-app` afterwards (only UIKit, Foundation, CoreFoundation, CoreGraphics, CoreText, QuartzCore, ImageIO).
- **iOS simulator: launch apps only after `simctl bootstatus -b`**; apps launched while the simulator boots are sent to the
  background when SpringBoard comes up.
- **IPAs are zipped with `ditto -c -k --norsrc --keepParent`**: `--sequesterRsrc` adds `__MACOSX/` resource forks.
- **rquickjs converts `Option::None` to `undefined`, not `null`**: compare native results with `== null` in JS.
- **Android, JNI**: cache classes and method ids in `JNI_OnLoad`; wrap every call from the (permanently attached) engine thread in
  a local frame (`java::with_env`), or local references pile up.
- **Android, text**: `StaticLayout` ignores `maxLines` without an ellipsis (clip mode counts lines itself and the view clips);
  build layouts from `PrecomputedText` with the same break strategy / hyphenation / text direction as its params, or it
  measures again. A layout of precomputed text does not draw `UnderlineSpan` / `StrikethroughSpan` (even ones set after
  `create`): runs carry a `TextLayouts.Decoration` marker and `TextBox` draws the lines. `Layout.Alignment` is relative to each
  paragraph's direction: absolute left / right alignment sets an `AlignmentSpan` per paragraph from its resolved direction.
- **Android, JNI event maps intern their keys from a fixed list** (`Java_dev_yf_Native_eventMap`): a new payload key must be
  added there or it is dropped silently.
- **Android, inputs**: `EditText` reconfiguration (input type, filters, single line) sets the same text again and fires
  `afterTextChanged`: only text that differs from the last known one counts as an edit, or JS values are rejected as stale.
  Soft keyboards edit through the input connection (`KeyConnection`: composing text, commits, deletes), hardware keys arrive
  as key events; both feed `onKeyPress`. A hardware Enter reports its down and its up — act on the down only and consume the up, or the up advances
  focus again in the next field; blur by focusing the (focusable, highlight-free) `YfView`, as `clearFocus()` hands focus to the
  first focusable view; set the typeface after the input type (password types reset it to monospace).
- **Android, keyboard avoidance**: the keyboard's height becomes bottom padding of the scroll around the focused input, and
  `ScrollBox.reveal` scrolls it into view; `requestRectangleOnScreen` cannot, as ScrollView ignores its padding when it decides
  whether a rectangle is visible. `YfView` is a scroll container (`setScrollContainer(true)`) so that windows without a soft
  input mode resize instead of panning: the system picks the mode at the first layout, before the app's content is mounted,
  and a mode set from code afterwards is overridden. Windows that ask for `adjustPan` are left to the system.
- **Android, drags that start on a child**: a pannable `Box` intercepts the move that exceeds the slop and starts its pan right
  there; that intercepted move never reaches its `onTouchEvent`, so starting the pan there is what makes a quick flick (few moves)
  pan and end at all.
- **Android, modal content placement**: `Box.place` lays a view out at its core frame directly, so the status bar offset of a
  non-translucent modal is a content view inside the dialog's container, not an offset applied to the root's layout; the dialog
  copies the activity's bar appearance (light / dark icons).
- **Android, nested scrolling**: scroll views enable nested scrolling, or an outer vertical ScrollView intercepts every vertical
  drag meant for an inner one.
- **Android, safe area**: `YfView` reports only the parts of the system bars / keyboard that overlap it (like UIKit's
  `safeAreaInsets`), so embedded views below a toolbar get no top inset. Overlays use the same overlap (`topInset()`).
- **Android, dev assets come over plain HTTP**, which apps block by default (API 28+): the dev host APK and `yf-dev.aar` set
  `usesCleartextTraffic`; the release AAR does not.
- **Android, activities**: create the decor view (`getDecorView()`) before asking for the insets controller; apps draw edge to edge
  and get the system bars through `safeArea()`.

- **`on*` props are event handlers to the JS renderer** (registered, sent to the core as a listener flag): settings must not
  be named `on...` on host elements (`List` sends `onEndReachedThreshold` as `endReachedThreshold`).
- **Platforms clamp scroll offsets to the content**: UIKit clamps `contentOffset` when `contentSize` shrinks, Android's
  ScrollView clamps `scrollBy` to the child's current size. The core grows the content before a `ScrollBy` and scrolls
  before shrinking it; Android sizes the content view at once in `setContentSize` (not at the next layout pass).
- **Scroll reports can predate the core's own scrolls**: each report carries how many `ScrollBy` / list `ScrollTo` the view
  has applied (`EngineControl::scroll(.., applied)`), and the core adds the ones still pending; otherwise a stale offset
  moves the window (and fires `onEndReached`) backwards.
- **`TextView` uses `UIViewContentMode::Redraw`**: list cells can get their text before their frame; without redrawing on
  resize the text stays drawn at zero size.
- **Unbound list cells still show their last item**: the core moves them out of sight (never with a NaN frame: cells that
  were never placed are skipped; UIKit throws on NaN geometry).
- **Solid**: a computation that throws stays stale and never runs again (no error boundary), so hot component proxies catch
  render errors and rethrow them from a microtask; the universal renderer's `render` disposer does not remove the inserted
  views (the runtime's `render` does).
- **rolldown CommonJS chunks define reassigned exports as non-configurable getters**: a module run again fills a fresh
  `exports` object and the hot runtime forwards the importers' object to it.
- **Hot proxies return accessors**, not nodes: runtime code that inspects what a component returned (`Stack` looks for a
  `<screen>`) must accept an accessor (`createMemo` over it).
- **UINavigationController breaks transitions started during another one**: stack changes that arrive while
  `transitionCoordinator()` is set wait for `didShow` and are applied on the next main-queue turn (the coordinator outlives
  the callback); only screens the controller actually had count as popped by the user.
- **`UIAlertController` asserts when its presentation controller's delegate is set**; a popover action sheet (iPad, Mac) hides
  its cancel action and runs it on a tap outside, so sheets without one get a hidden cancel answering "dismissed".
- **Catalyst: a mouse drag does not scroll a UIScrollView** (Mac behavior); test scrolling with trackpad-style scroll wheel
  events (with phases), which also produce drag and momentum callbacks. Mouse drags do reach views as touches.
- **Catalyst's Mac idiom presents page / form sheets as macOS sheets** at a default size wider than a 390pt window: they get
  the window's size as `preferredContentSize`. Standalone apps have a white root view (UIKit windows are black).
- **Development hosts never restore windows**: `yf run` launches the Catalyst host with `-ApplePersistenceIgnoreState YES`;
  after a crash, macOS would otherwise show a modal "reopen windows?" alert over it.
- **Android widgets**: `MediaPlayer.setPlaybackParams` starts playback (set the rate only while playing) and a paused player draws nothing until
  it seeks; `WebView` JavaScript bridge calls arrive on a binder thread (post to the UI thread).
- **Synthetic clicks on the Simulator / Catalyst windows go to whatever macOS window is on top at that point**: floating windows
  of other apps (e.g. a Slack huddle bar at window level 1001) silently swallow clicks over a band of the app. When a tap does
  nothing, list the windows at the point (`CGWindowListCopyWindowInfo`) before suspecting hit testing.
- **Tailwind variant order decides between variants**, not their order in the string: `odd:bg-a last:bg-b` gives an odd last row
  `bg-a` (Tailwind sorts `odd` after `last`), exactly as in CSS.
- **iOS clamps shared corner radii itself** (`rounded-full` is 9999): Core Animation does not reliably clamp a `cornerRadius`
  over half the short side, so `decor.rs` clamps it and clamps again when the size changes.
- **Android, negative letter spacing**: Minikin adds half the spacing on both sides of each glyph and trims the halves at line edges,
  so a line is wider than its sum of advances; `TextLayouts.Prepared.desired` adds the largest negative spacing back, or text laid
  out at its natural width wraps (`tracking-tight`).
- **Not supported / not verifiable here**: `I18nManager.swapLeftAndRightInRTL(false)` (left / right always swap right to left);
  after switching direction at runtime UIKit keeps an existing back button's chevron; Android has no skew transforms (as in React Native); the iOS simulator cannot
  inject edge swipes (interactive back is untested there); Android's stylus handwriting mode (on by default on recent
  emulators) intercepts taps on inputs — turn it off with `adb shell settings put secure stylus_handwriting_enabled 0`.

## Verification after changes

1. `cargo test --workspace`, plus fmt and clippy as listed above (including the Catalyst target and `cargo xtask clippy-android`).
2. Changes to the JS runtime, engine or core: run `yf trace` and check item counts and timings.
3. Changes to the platform layer: launch with `yf run`, check screenshots and simulate input where needed (the terminal needs the Screen
   Recording and Accessibility permissions; while the screen is locked the frontmost app is `loginwindow` and clicks fail, but window
   screenshots still work). iOS simulator: `xcrun simctl io booted screenshot`, `xcrun simctl ui booted appearance dark`.
   Android: `adb exec-out screencap -p`, `adb shell input tap|swipe|text|keyevent` (all work with the screen locked).
