---
title: Getting started
description: Create an app with nsh init, read through its App.tsx, run it on Mac Catalyst, the iOS simulator or Android, and edit it with hot reload.
order: 3
demo: guide-getting-started
---

# Getting started

This page creates a new app, explains the code `nsh init` writes, and runs it with hot reload. It assumes `nsh` is installed and on your `PATH` ([Installation](./installation.md)); in a checkout, use `./target/debug/nsh` instead.

## Create an app

```sh
nsh init my-app
```

`nsh init` creates the directory if it is missing and writes:

| File | What it is |
|---|---|
| `App.tsx` | The entry module: renders the app with `mount` |
| `nsh.json` | The app's name, id, version and build number ([Project structure](./project-structure.md#nshjson)) |
| `tsconfig.json` | TypeScript settings that resolve `nsh` to the declarations in `.nsh/types` |
| `.gitignore` | Ignores `.nsh/` and `out/` |
| `.nsh/types/` | Generated declarations of every nsh module, for your editor |

The display name defaults to the directory's name; pass `--name "My App"` to choose another. The id becomes `com.example.<name>`: change it in `nsh.json` before you ship. Files that already exist are kept, so you can run `nsh init` in a directory that has some of them.

## The entry module

```tsx
import { mount, signal } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  const count = signal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</text>
      <view
        onPress={() => count.update((c) => c + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <text style={{ color: "white", fontSize: 17 }}>Tap</text>
      </view>
    </SafeAreaView>
  );
}

mount(App);
```

Line by line:

- **`import { mount, signal } from "nsh"`**: `nsh` is the core module: [signals](./reactivity.md), `mount`, and the other reactive primitives. It is served by the native runtime, not bundled.
- **`import { SafeAreaView } from "nsh/ui/mobile"`**: the mobile UI set. [`SafeAreaView`](../components/safe-area-view.md) pads its content so it stays clear of the notch, the status bar and the home indicator.
- **`function App()`** is a component: a function that returns JSX. It runs **once**. `signal(0)` makes a value that changes over time.
- **`{count()}`** reads the signal inside JSX. The compiler sees a call and makes this text follow the signal: when `count` changes, the runtime updates this one text, nothing else.
- **`<view onPress>`** is a native view that reacts to presses; `activeOpacity` fades it while pressed, natively. Lowercase tags (`view`, `text`, `image`, `scroll`, `input`, ...) are [host elements](../elements/index.md).
- **`style={{ … }}`** takes React Native–style properties. Layout is flexbox, with React Native's defaults (children stack in a column) ([Layout](./layout.md)).
- **`mount(App)`** renders the component into the app's surface.

> [!IMPORTANT]
> Text must be inside `<text>`. A bare string in a `<view>` (`<view>Hello</view>`) is not displayed.

## Run it

`nsh run` starts a dev server, builds the app, launches a development host that loads it, and keeps rebuilding while you edit. Logs from the app (`console.log`, errors) appear in the terminal.

### Mac Catalyst (default)

```sh
nsh run my-app/App.tsx
```

The app opens in an iPhone-sized window (390×844 points) on your Mac. This is the fastest loop: no simulator to boot. It uses UIKit like iOS, so what you see is what an iPhone shows, apart from Mac-specific behavior such as scrolling with a mouse wheel.

### iOS simulator

```sh
nsh run my-app/App.tsx --target ios
nsh run my-app/App.tsx --target ios --device "iPhone 16 Pro"
```

`nsh` uses a booted simulator, or boots the newest iPhone if none is running, and installs the development host into it. `--device` picks a simulator by name or UDID.

### Android

```sh
nsh run my-app/App.tsx --target android
```

Start an emulator or connect a device first (`adb devices` lists them; nsh uses the first one). `nsh` installs a development host APK (rebuilt only when the runtime changes) and forwards the dev server's port to the device with `adb reverse`, so it works over USB without a shared network.

## Edit with hot reload

Leave `nsh run` running and change the label in `App.tsx`:

```tsx
<text style={{ color: "white", fontSize: 17 }}>Add one</text>
```

Save. The CLI rebuilds and sends a patch; the host runs the changed module again and renders again only the components whose code changed. Here that is `App`, which owns the `count` signal, so the count starts over. State survives when you edit a component that does not own it. Move the button into a component of its own:

```tsx
function TapButton(props: { onPress: () => void }) {
  return (
    <view onPress={props.onPress} activeOpacity={0.6} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <text style={{ color: "white", fontSize: 17 }}>Tap</text>
    </view>
  );
}
```

Use `<TapButton onPress={() => count.update((c) => c + 1)} />` in `App`, tap a few times, then change `TapButton`'s color: the button updates and the count stays.

Hot replacement works per top-level component: a function with a capitalized name whose body contains JSX. A component renders again when its own code changes, or when the module's code outside components changes (a constant it uses, a helper). If an edit cannot be applied in place (for example, it imports an nsh module the running app did not import yet, or changes `nsh.json`'s capabilities), the host reloads the whole app.

### Errors

A build error is printed in the terminal and shown on the device as a red overlay with the file, line and column; the app keeps running the last good build. The overlay disappears with the next successful build (tap it to dismiss it earlier). An exception thrown at run time is shown the same way, with its stack mapped back to your `.tsx` source.

## Try it in the terminal

`nsh trace` runs an app headless (no device) and prints the native view operations it produces and their timings, then taps the first pressable view twice:

```sh
nsh trace my-app/App.tsx
```

It is a quick way to check that an app starts without errors, and to see how much work an interaction causes ([Testing and trace](./testing-and-trace.md)).

## Next steps

- [Project structure](./project-structure.md): what goes in `nsh.json`, assets, modules.
- [JSX and components](./jsx-and-components.md): props, children, control flow.
- [Reactivity](./reactivity.md): signals, computed values and effects.
- [Packaging](./packaging.md): build an installable `.app`, `.ipa` or `.apk`.
