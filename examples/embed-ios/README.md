# Embedding nsh in an iOS app

`main.swift` is an existing UIKit app that shows a nsh app below its own
native views.

1. Add `dist/nsh/embed/ios/NativeShell.xcframework` (or `NativeShellDev.xcframework` for
   development builds) to the app target. Swift code then `import NativeShell`.
2. Build the nsh bundle and add the output folder to the app's resources as
   `nsh`: `nsh build App.tsx --release --out nsh`.
3. Create the view with `nsh_app_create`, add `nsh_app_view(app)` to a view
   hierarchy and call `nsh_app_load_bundle(app, path)` with the path of
   `nsh/bundle.qbc`. Call `nsh_app_destroy(app)` when done.

With `NativeShellDev.xcframework`, `nsh_app_connect_dev_server(app, "ws://127.0.0.1:8790/ws")`
loads from `nsh dev` instead and reloads on every change.

All functions must be called on the main thread. The C API is in
`crates/nsh-ffi/include/nsh.h`.
