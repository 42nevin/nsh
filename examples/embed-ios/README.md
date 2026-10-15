# Embedding nsh in an iOS app

`main.swift` is an existing UIKit app that shows a nsh app below its own
native views.

1. Add `dist/nsh/embed/ios/NativeShell.xcframework` (or `NativeShellDev.xcframework` for
   development builds) to the app target (`cargo xtask ios-embed` builds both). Swift code then
   `import NativeShell`.
2. Build the nsh bundle and add the output folder to the app's resources as
   `nsh`: `nsh build App.tsx --release --out nsh`.
3. Create an `NshAppView` (a `UIView`), add it to a view hierarchy and call
   `try app.loadBundle(at:)` with the URL of `nsh/bundle.qbc`. The app stops when the view is deallocated.

Signed packages (`nsh publish`; see **Signed packages** in AGENTS.md), e.g. updates the app downloads:

```swift
let trust = try NshTrust(stateDirectory: applicationSupport.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)                 // root.pub from `nsh keygen`
let info = try await app.loadPackage(at: downloaded, trust: trust)
```

`loadPackage` verifies off the main thread and throws why a package was refused (the running app
stays). Bytecode files only load from inside the app bundle: downloaded code must come as a signed
package (or JS source).

With `NativeShellDev.xcframework`, `try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)`
loads from `nsh dev` instead and reloads on every change.

`NshAppView` is main-actor isolated; `NshTrust` is thread-safe. The host view controller can forward
the app's status bar: `NshAppView.preferredStatusBarStyle`, `.prefersStatusBarHidden`,
`.preferredStatusBarUpdateAnimation`.
