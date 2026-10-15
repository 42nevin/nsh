# JS modules

Two layers. `nsh` and `nsh/<module>` (`js/runtime`) are the primitives and the environment that every UI set builds
on. `nsh/ui/<set>` (`js/ui/<set>`) is a whole UI for one form factor: `nsh/ui/mobile` has everything that is mobile UI or
interaction, natively backed (navigation stacks, modals, the status bar, system dialogs) or JS (the components, one
look on iOS and Android); a desktop set will be a separate one (`nsh/ui/desktop`), not an abstraction over both.
Shared vs one form factor decides the layer, not JS vs native.

`nsh-bundler/src/embedded.rs` lists the embedded files per root (`runtime`, `ui/mobile`) with the specifiers apps import;
files import their own root's files relatively (`./navigation`) and the runtime's internals as `nsh/_core` (embedded
files only; apps cannot). `.nsh/types` has one declaration file per embedded file (`nsh/list.d.ts`,
`nsh/ui/mobile/navigation.d.ts`) and `nsh/ui/mobile.d.ts` re-exporting its set, found through the `nsh/*` path of the
app's `tsconfig.json`; files nsh no longer generates are removed. `nsh` itself only has rendering and the types; there
is no catch-all module, and moved APIs get no compatibility re-exports.

| Module | Exports |
|---|---|
| `nsh` | `render`, `root`, `YNode`; the types of styles, element props, events and `JSX` |
| `nsh/animated` | `createNativeValue`, `NativeValue`, `NativeNode`, `Animated`, `Easing`, `LayoutAnimation` |
| `nsh/device` | `windowSize()`, `colorScheme()`, `fontScale()`, `Platform`, `PixelRatio`, `I18nManager` |
| `nsh/accessibility` | `AccessibilityInfo`, `screenReaderEnabled()`, `reduceMotion()` |
| `nsh/list` | `List` |
| `nsh/canvas` | `Canvas`, `Path2D`, `CanvasRenderingContext2D`, `CanvasGradient` |
| `nsh/image` | `Image.getSize`, `Image.prefetch` |
| `nsh/gesture` | `PanResponder` |
| `nsh/webview`, `nsh/video` | `WebView`, `Video` |
| `nsh/native` | `nativeComponent(name, { props, events })`: a component for a native component the embedding app registered |
| `nsh/ios` | `isAvailable`, `InputAccessoryView` |
| `nsh/android` | `isAvailable`, `Toast`, `BackHandler`, `setStatusBarBackgroundColor` |
| `nsh/renderer` | what compiled JSX calls (`createElement`, `insert`, `spread`, ...; the Solid compiler's `moduleName`) |
| `nsh/ui/mobile` | `Stack`, `Screen`, `createNavigator`, `Modal`; `SafeAreaView`, `useSafeAreaInsets`, `safeArea()`; `KeyboardAvoidingView`, `keyboard()`, `Keyboard.dismiss`; `StatusBar`, `Alert`, `ActionSheet` (a list dialog on Android) |

- **Platform modules** are importable on both platforms (one bundle runs on both); on the other one their calls do
  nothing and their components render nothing. `isAvailable` tells which.
- **Platform-only props** are grouped: `ios={{…}}` / `android={{…}}` / `desktop={{…}}` (macOS, Windows, Linux) on
  elements and components (`IOSProps` / `AndroidProps` / `DesktopProps` in `types.ts`, extended per element). The
  renderer (`platformProps` in `_core.ts`) sends only the current platform's object, each key as a prop of its own (keys removed from the object are cleared); the core knows them by
  those names. Android: `ripple`, `elevation`, `accessibilityLiveRegion`, Modal `statusBarTranslucent`, StatusBar
  `backgroundColor`, Alert `cancelable`. iOS: input `inputAccessoryViewID` / `textContentType`, Screen `headerLargeTitle` /
  `headerBackTitle`, Modal `presentationStyle` / `supportedOrientations`, WebView `allowsBackForwardNavigationGestures`.
- Module-level code that must always run (the engine's hooks `__nsh_dispatch` / `__nsh_env`, the root's direction, the
  Tailwind theme) lives in `_core.ts`; feature modules only run when imported (production bundles tree-shake the rest).
  `_core.ts` recognizes native animation values by their `__nv` field, so it does not import `nsh/animated`.

## Where new things go

- A primitive or environment both UI sets need: a module of `js/runtime` (`nsh/<module>`), listed in
  `crates/nsh-bundler/src/embedded.rs`.
- Mobile UI or interaction, natively backed or JS: `js/ui/mobile` (`nsh/ui/mobile`); components drawn with the
  primitives go to `js/ui/mobile/components/` (see its README).
- Something only one platform has: `nsh/ios` / `nsh/android`, or a prop in `ios={{…}}` / `android={{…}}` /
  `desktop={{…}}`.
- Never a compatibility re-export when something moves, never a React Native compatibility wrapper.
