# Mobile UI components

This directory holds the components of `nsh/ui/mobile`, written in TypeScript (TSX + Solid): buttons, switches, sliders,
sheets, tab bars and the like. They are the mobile UI set's own components, drawn with nsh's primitives so that they
look and behave the same on iOS and Android.

- **One file per component** (`button.tsx`), exported from `../index.ts` (`export * from "./components/button"`) and
  listed in `crates/nsh-bundler/src/embedded.rs` (`UI_MOBILE`, with its extension): the bundler embeds the file and
  compiles it like app code (Solid's universal JSX).
- **Primitives only**: `<view>`, `<text>`, `<image>`, `Canvas`, classes, native animations (`nsh/animated`). Never wrap a
  platform widget (UISwitch, SeekBar, ...): nsh has one look on both platforms. Press feedback is native (`active:`
  classes, `activeOpacity`, `underlayColor`), never a JS round trip.
- **Imports**: `nsh` for types and rendering, `nsh/<module>` for primitives, `solid-js`. Files of this set import each
  other relatively; `nsh/_core` (the runtime's internals) is reserved for what needs native support.
- **Explicit types on every export** (TypeScript's isolated declarations generate `.nsh/types` from these files; a
  nsh-bundler test enforces it). Document every prop.
- **Styling**: Tailwind classes for the look (with `dark:` variants), a `class` prop appended last so apps can override,
  and `style` passed through. Text classes on a view style the texts inside it.
- **Accessibility**: a role, a label and the state (`disabled`, `checked`, ...) on every interactive component.
- **Try it** in `examples/gallery` (the Components demo shows the set).
