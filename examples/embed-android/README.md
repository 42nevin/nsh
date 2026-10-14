# Embedding yf in an Android app

`MainActivity.java` is an existing Android app that shows a yf app below
its own native views.

1. Add `dist/yf/embed/android/yf.aar` (or `yf-dev.aar` for development
   builds) to the app, e.g. in Gradle: `implementation(files("libs/yf.aar"))`.
2. Build the yf bundle into the app's assets:
   `yf build App.tsx --release --out app/src/main/assets/yf`.
3. Create a `dev.yf.YfView`, add it to a layout and call
   `loadAssetBundle("yf")`. Call `destroy()` when the activity goes away.

With `yf-dev.aar`, `connectDevServer("ws://<mac ip>:8790/ws")` loads from
`yf dev` instead and reloads on every change (for an emulator, run
`adb reverse tcp:8790 tcp:8790` and use `ws://127.0.0.1:8790/ws`). Assets then
come from the dev server over plain HTTP, so `yf-dev.aar` declares
`android:usesCleartextTraffic="true"`, which the manifest merger applies to
the app; a network security config of the app may override it.

Keyboard: `YfView` scrolls focused inputs clear of the keyboard itself.
Leave `android:windowSoftInputMode` unset or use `adjustResize`; with
`adjustPan` the system moves the whole window instead.

The view reports the safe area and keyboard as the parts of the system bars
and the keyboard that overlap it, so an app embedded below other views gets
no top inset.

`YfView` needs the `INTERNET` permission (declared by the AAR) and API 28+.
