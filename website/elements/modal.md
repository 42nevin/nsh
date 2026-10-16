---
title: modal
description: "The modal element and the Modal component: content presented over everything else, as a full screen, a sheet or a dialog."
order: 11
---

# modal

A modal presents content over the rest of the app in its own native window layer: a presented view controller on iOS
and Mac Catalyst, a Dialog on Android. It is used for full-screen flows, sheets and dialogs. The system's own gestures
(swiping a sheet down, the Android back button) work, and are decided natively.

```tsx
import { signal } from "nsh";
import { Button, Modal, SafeAreaView } from "nsh/ui/mobile";

function Example() {
  const open = signal(false);
  return (
    <view style={{ flex: 1, justifyContent: "center", padding: 24 }}>
      <Button title="Open" onPress={() => open.set(true)} />
      <Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12, backgroundColor: "#ffffff" }}>
          <text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</text>
          <Button title="Close" onPress={() => open.set(false)} />
        </SafeAreaView>
      </Modal>
    </view>
  );
}
```

## `Modal` and `<modal>`

Use `Modal` from [`nsh/ui/mobile`](../components/navigation.md). It adds what the bare tag lacks:

- a `visible` prop: the modal is presented while `visible` is not `false`, and dismissed when it becomes `false`;
- the modal's safe-area insets, which `SafeAreaView` and `useSafeAreaInsets()` inside it read.

The bare `<modal>` tag is presented for as long as it is mounted (wrap it in an [`<if>`](./control-flow.md) to show and
hide it) and gives its content no insets. Importing `Modal` makes `<modal>` in that file the component.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `visible` | `boolean` | `true` | `Modal` only: presented while true. |
| `animationType` | `"slide" \| "fade" \| "none"` | `"slide"` | How the modal appears. |
| `transparent` | `boolean` | `false` | The background shows what is behind (for dialogs drawn by the app). |
| `dismissible` | `boolean` | `false` | The user closes it natively (iOS sheet swiped down, Android back button), then `onRequestClose` and `onDismiss` follow. Without it, those gestures only ask to close. |
| `onRequestClose` | `() => void` | | The user asked to close: set `visible` to false to close it. |
| `onShow` | `() => void` | | The modal finished appearing. |
| `onDismiss` | `() => void` | | The modal finished disappearing. |
| `style` | `StyleProp<ViewStyle>` | | The modal's root view. |
| `children` | `JSX.Element` | | The content. |

| `ios` key | Type | Default | Description |
|---|---|---|---|
| `presentationStyle` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | `pageSheet` and `formSheet` keep what is behind visible; `formSheet` is a smaller centered sheet on large screens. |
| `supportedOrientations` | `("portrait" \| "portrait-upside-down" \| "landscape" \| "landscape-left" \| "landscape-right")[]` | the presenter's | The orientations the modal allows. |

| `android` key | Type | Default | Description |
|---|---|---|---|
| `statusBarTranslucent` | `boolean` | `false` | The content reaches under the status bar; otherwise it starts below it. |

## Controlled by default

A modal is controlled: when the user swipes a sheet down or presses back, nsh asks with `onRequestClose` and the modal
stays until you set `visible` to `false`. This lets you confirm before closing. Since the main thread never waits for
JS, the decision cannot be made in the handler; `dismissible` tells the platform beforehand that it may close the
modal by itself:

```tsx
<Modal visible={open()} dismissible ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
  …
</Modal>
```

With `dismissible`, the sheet follows the finger and closes natively; `onRequestClose` then tells you to update your
state.

## A dialog

A transparent modal with a fade draws its own dialog over a dimmed background:

```tsx
<Modal visible={confirm()} transparent animationType="fade" onRequestClose={() => confirm.set(false)}>
  <view style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <view style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <text style={{ fontSize: 20, fontWeight: "700" }}>Delete this item?</text>
      <Button title="Delete" onPress={remove} />
    </view>
  </view>
</Modal>
```

For a system alert or action sheet, use `Alert` and `ActionSheet` from `nsh/ui/mobile` instead
([alert](../components/alert.md), [action sheet](../components/action-sheet.md)). For a sheet drawn by the app, see
[`Sheet`](../components/sheet.md).

## Ref methods

None.

## Platform notes

- Android: the back button goes to the topmost modal first, before stacks and `BackHandler` listeners.
- `presentationStyle` and `supportedOrientations` are iOS only and ignored on Android.
- Without `statusBarTranslucent`, an Android modal's content starts below the status bar.
