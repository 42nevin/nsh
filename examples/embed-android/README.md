# Embedding nsh in an Android app

`MainActivity.java` is an existing Android app that shows a nsh app below
its own native views.

1. Add `dist/nsh/embed/android/nsh.aar` (or `nsh-dev.aar` for development
   builds) to the app, e.g. in Gradle: `implementation(files("libs/nsh.aar"))`.
2. Build the nsh bundle into the app's assets:
   `nsh build App.tsx --release --out app/src/main/assets/nsh`.
3. Create a `com.longbridge.nsh.NshView`, add it to a layout and call
   `loadAssetBundle("nsh")`. Call `destroy()` when the activity goes away.

With `nsh-dev.aar`, `connectDevServer("ws://<mac ip>:8790/ws")` loads from
`nsh dev` instead and reloads on every change (for an emulator, run
`adb reverse tcp:8790 tcp:8790` and use `ws://127.0.0.1:8790/ws`). Assets then
come from the dev server over plain HTTP, so `nsh-dev.aar` declares
`android:usesCleartextTraffic="true"`, which the manifest merger applies to
the app; a network security config of the app may override it.

Keyboard: `NshView` scrolls focused inputs clear of the keyboard itself.
Leave `android:windowSoftInputMode` unset or use `adjustResize`; with
`adjustPan` the system moves the whole window instead.

The view reports the safe area and keyboard as the parts of the system bars
and the keyboard that overlap it, so an app embedded below other views gets
no top inset.

`NshView` needs the `INTERNET` permission (declared by the AAR) and API 28+.
