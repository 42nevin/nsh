# native-shell (nsh)

A React Native–like UI framework: apps are written in TSX with SolidJS, run in QuickJS, are laid out by Taffy and
render with the system's native views (UIKit on iOS / Mac Catalyst, Android views). The toolchain is a single `nsh`
binary written in Rust: no Node.js, and no Rust toolchain for app developers.

## Running the examples

| Example | What it shows |
|---|---|
| `examples/gallery/App.tsx` | A demo index covering what nsh supports: lists (recycling, columns, sticky sections, an inverted chat, horizontal lists), navigation (header options, modals, the back button), forms and the keyboard, controls, gestures, native animations, images, styles, text, web view and video, environment / timers / fetch |
| `examples/counter/App.tsx` | The smallest app |
| `examples/embed-ios`, `examples/embed-android` | nsh inside existing native apps (see their READMEs) |

Every `nsh run` starts a dev server, launches the app and updates it on every save (hot module replacement: only the
edited components render again, the rest of the app keeps its state). Logs and errors appear in the terminal.

### With the distribution (no Rust needed)

`dist/nsh` (made by `cargo xtask dist`) contains the CLI and prebuilt runtimes:

```sh
dist/nsh/bin/nsh run examples/gallery/App.tsx                      # Mac Catalyst window (390×844), macOS only
dist/nsh/bin/nsh run examples/gallery/App.tsx --target ios         # iOS simulator (boots one if none is running)
dist/nsh/bin/nsh run examples/gallery/App.tsx --target android     # Android emulator or device
```

Requirements: Xcode for the Mac and iOS targets; for Android, the Android SDK (`ANDROID_HOME`, or
`~/Library/Android/sdk`) with a running emulator or a device connected over `adb`. Pick a simulator with
`--device "iPhone 16 Pro"`.

### From a checkout

Build the CLI and the development host of the target you want once (and again after changing Rust code):

```sh
cargo build -p nsh-cli
cargo build -p nsh-host --target aarch64-apple-ios-macabi   # for the Mac Catalyst window
cargo build -p nsh-host --target aarch64-apple-ios-sim      # for --target ios
cargo xtask android --dev                                   # for --target android
```

Then run an example:

```sh
./target/debug/nsh run examples/gallery/App.tsx
./target/debug/nsh run examples/gallery/App.tsx --target ios
./target/debug/nsh run examples/gallery/App.tsx --target android
```

### Other ways to run an app

`nsh` below is `dist/nsh/bin/nsh` or `./target/debug/nsh`.

```sh
nsh dev examples/gallery/App.tsx                                    # dev server only; prints the URLs for hosts on this machine and the LAN
nsh trace examples/counter/App.tsx                                  # headless: prints the native view operations and timings
nsh package examples/gallery/App.tsx --target ios-simulator --run   # an installable app (.app / .ipa / .apk)
nsh build examples/counter/App.tsx --release                        # bundle.js / bundle.qbc for embedding
```

## A new app

```sh
nsh init my-app
nsh run my-app/App.tsx
```

`nsh init` creates `App.tsx`, `nsh.json` (name, id, version, icon) and a `tsconfig.json`; type declarations for
editors are kept in `my-app/.nsh/types`, so TypeScript checking works without `node_modules`.

Contributors: see [AGENTS.md](AGENTS.md) for the architecture, commands and conventions.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
