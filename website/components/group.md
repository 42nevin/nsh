---
title: Group
description: Settings-style rows in a rounded box, divided by hairlines, with an optional caption and footnote.
order: 4
demo: ui-group
---

# Group

`Group` shows rows in a rounded box with a hairline between them, like a section of a settings screen. Each JSX child
becomes one row. You can add a short caption above the box and an explanation below it.

```tsx
import { Group } from "nsh/ui/mobile";
```

## Usage

```tsx
import { mount, signal } from "nsh";
import { Group } from "nsh/ui/mobile";

function App() {
  const sounds = signal(true);
  return (
    <view style={{ flex: 1, backgroundColor: "#f5f5f5", padding: 16, paddingTop: 48 }}>
      <Group
        header={<text>NOTIFICATIONS</text>}
        footer={<text>Sounds play for new messages only.</text>}
      >
        <text>Show previews</text>
        <view
          onPress={() => sounds.set(!sounds())}
          style={{ flexDirection: "row", justifyContent: "space-between" }}
        >
          <text>Sounds</text>
          <text>{sounds() ? "On" : "Off"}</text>
        </view>
      </Group>
    </view>
  );
}

mount(App);
```

## Rows

Group wraps each child in a row with 16 pt horizontal and 12 pt vertical padding and a minimum height of 44 pt, the
smallest comfortable touch target. The content is centered vertically. Every row except the first has a 1 pt divider
at its top.

A row is whatever you pass, so put a `<view>` with `flexDirection: "row"` in it to place a label and a value, and an
`onPress` on that view to make the row pressable. When a child is added or removed, for example with
[`<if>` or `<each>`](../elements/control-flow.md), the group updates the rows and dividers.

```tsx
<Group header={<text>DEVICES</text>}>
  <each of={devices()}>
    {(device) => (
      <view style={{ flexDirection: "row", justifyContent: "space-between" }}>
        <text>{device().name}</text>
        <text style={{ color: "#737373" }}>{device().status}</text>
      </view>
    )}
  </each>
</Group>
```

> [!TIP]
> Group renders every row. For long or unbounded lists, use [ListView](./list-view.md) with `variant="inset"`,
> which has the same rounded-box look and only creates the rows that are near the screen.

## Header and footer

The header is placed 16 pt in from the left, in small (`text-sm`) muted text. The footer is in extra-small
(`text-xs`) muted text. There is an 8 pt gap between the header, the box and the footer. Both are optional.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `header` | `JSX.Element` | none | Above the box: a short caption. |
| `footer` | `JSX.Element` | none | Below the box: an explanation. |
| `children` | `JSX.Element` | none | The rows, one per child. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the group's root view (the column holding the header, box and footer). |

`Group` has no events.

## Theming

| Part | Theme key |
|---|---|
| Box | `group_box.background`, corner radius `radius.lg` |
| Row text color | `group_box.foreground` |
| Dividers | `border` |
| Header and footer text | `muted.foreground` |

See [Theming](./index.md#theming).

## Accessibility

Group adds no roles or labels. Each row's content is read as you wrote it. If a row is pressable, give its view an
`accessibilityRole` (for example `"button"`) and, if needed, an `accessibilityLabel`. See
[Accessibility](../docs/accessibility.md).

## Related

- [Card](./card.md): one surface with a header and footer.
- [ListView](./list-view.md): virtualized rows with the same inset look.
- [Stepper](./stepper.md): a common control inside a row.
