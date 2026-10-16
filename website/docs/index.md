---
title: Introduction
description: What native-shell is, how an app runs, and how it compares with other cross-platform frameworks.
order: 1
---

# Introduction

native-shell (nsh for short) is a framework for building iOS and Android apps in TypeScript and JSX. Your app's code runs in an embedded JavaScript engine, but everything else is native: the reactive graph, the renderer, layout, text and every module your app imports are written in Rust, and the screen is made of the platform's own views (UIKit on iOS and Mac Catalyst, Android `View`s on Android). There is no WebView and no canvas that redraws the whole UI.

```tsx
import { mount, signal } from "nsh";

function Counter() {
  const count = signal(0);
  return (
    <view style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</text>
      <view
        onPress={() => count.update((n) => n + 1)}
        activeOpacity={0.6}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <text class="text-white text-lg">Tap</text>
      </view>
    </view>
  );
}

mount(Counter);
```

`<view>` becomes a `UIView` or an Android `ViewGroup`, `<text>` a native text view, and the button's press feedback (`activeOpacity`) runs on the main thread without a round trip to JavaScript. When you tap, `count.update` changes one signal, and only the text that reads `count()` is updated: the component function does not run again.

## How an app runs

An nsh app is a set of `.tsx` / `.ts` modules with an entry, usually `App.tsx`. The toolchain is a single `nsh` binary:

1. **Build.** `nsh` bundles your modules (with rolldown and Oxc) and compiles the JSX with its own compiler. Imports of `nsh` and `nsh/*` stay imports: the runtime serves them natively, so the bundle holds only your code. The bundle is compiled to QuickJS bytecode.
2. **Run.** On the device, the engine thread runs the bytecode in [QuickJS](https://github.com/quickjs-ng/quickjs). Compiled JSX calls the native renderer directly; changes are collected during one JavaScript turn, laid out with [Taffy](https://github.com/DioxusLabs/taffy) (flexbox and CSS grid), and handed to the main thread as one batch of view operations per frame.
3. **Render.** The main thread applies the batch to native views. Text is measured and laid out once on the engine thread (CoreText on Apple platforms, `StaticLayout` on Android) and only drawn on the main thread.

Because JavaScript only describes the UI and handles events, the work done in the interpreter is small. Parsing styles, resolving Tailwind classes, diffing, layout and text all happen in Rust.

> [!NOTE]
> The main thread never waits for JavaScript. Questions the system asks synchronously (may this input accept another character, may the user swipe back, may this sheet close) are answered natively from props you set beforehand, such as `maxLength`, `preventBack` or `dismissible`. Events reach your code afterwards.

## What you get

- **Native views.** Embedding a native map, web view or video player has no layering problems, accessibility uses the platform's own tree, and text input is the system's.
- **Fine-grained reactivity.** [Signals](./reactivity.md) like Solid's, implemented natively. Components run once; only the expressions that read a changed signal run again.
- **React Native–style styles** with CSS additions: flexbox with React Native's defaults, CSS grid, box shadows, gradients, filters, transitions ([Styling](./styling.md)).
- **Tailwind CSS**, resolved natively: `class="flex-row gap-2 p-4 dark:bg-zinc-900 active:opacity-80"` ([Tailwind](./tailwind.md)).
- **A mobile UI set** (`nsh/ui/mobile`) whose components are written in Rust and look the same on iOS and Android: [Button](../components/button.md), [Card](../components/card.md), [ListView](../components/list-view.md), [navigation](../components/navigation.md), and more.
- **One binary toolchain.** No Node.js, no `node_modules`, and no Rust toolchain for app developers. `nsh init` writes TypeScript declarations into your project so editors type-check the app.
- **Hot module replacement.** `nsh run` rebuilds on save and patches the running app; only the components you edited render again.
- **Signed packages.** Apps that embed nsh can download and verify app bundles signed with your release key ([Signed packages](./signed-packages.md)).

## Where it runs

| Platform | Minimum | Notes |
|---|---|---|
| iOS | iOS 15.0 | Devices and the simulator |
| Mac Catalyst | – | Used as an iPhone-sized development window on the Mac (`nsh run`) |
| Android | API 28 (Android 9) | `arm64-v8a` and `x86_64` |

nsh apps can be packaged as standalone apps (`nsh package`) or embedded in existing native apps as a view ([iOS](./embedding-ios.md), [Android](./embedding-android.md)).

## How it compares

React Native also renders native views, but its renderer and React's reconciler run in JavaScript, and its toolchain is built on Node.js and the platforms' build systems. nsh keeps only your app code in JavaScript: the renderer, the reactive graph and the modules are native, and updates are fine-grained instead of re-rendering components and diffing a virtual tree. Flutter draws every pixel itself with its own widget set, so its controls imitate the platform's rather than being them; nsh uses the system's views, text engines and input. WebView-based frameworks (Capacitor, Cordova) render HTML in a browser engine; nsh has no DOM and no browser engine. See [Comparison with React Native](./react-native-comparison.md) for a detailed mapping of APIs.

## Next steps

- [Installation](./installation.md): get the `nsh` command and the platform tools.
- [Getting started](./getting-started.md): create an app and run it on a simulator.
- [JSX and components](./jsx-and-components.md) and [Reactivity](./reactivity.md): the two ideas everything else builds on.
