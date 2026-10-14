# Embedding yf in an iOS app

`main.swift` is an existing UIKit app that shows a yf app below its own
native views.

1. Add `dist/yf/embed/ios/YF.xcframework` (or `YFDev.xcframework` for
   development builds) to the app target. Swift code then `import YF`.
2. Build the yf bundle and add the output folder to the app's resources as
   `yf`: `yf build App.tsx --release --out yf`.
3. Create the view with `yf_app_create`, add `yf_app_view(app)` to a view
   hierarchy and call `yf_app_load_bundle(app, path)` with the path of
   `yf/bundle.qbc`. Call `yf_app_destroy(app)` when done.

With `YFDev.xcframework`, `yf_app_connect_dev_server(app, "ws://127.0.0.1:8790/ws")`
loads from `yf dev` instead and reloads on every change.

All functions must be called on the main thread. The C API is in
`crates/yf-ffi/include/yf.h`.
