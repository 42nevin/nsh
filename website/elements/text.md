---
title: text
description: "The element that draws text: nested spans, links, line limits, font scaling, inline views and text layout events."
order: 3
demo: el-text
---

# text

`<text>` draws a paragraph. It is the only element that shows strings: a string child of a `<view>` draws nothing. The
paragraph is typeset natively (CoreText on iOS and Mac Catalyst, `StaticLayout` on Android) and measured on the engine
thread, so the main thread only draws it.

```tsx
<text style={{ fontSize: 16, lineHeight: 24, color: "#1f2328" }}>
  Read the <text style={{ fontWeight: "700" }}>terms</text> before you continue.
</text>
```

## Spans

A `<text>` inside a `<text>` is a span of the same paragraph, not a separate view. Spans inherit the text styles of the
paragraph (`color`, `fontSize`, `fontWeight`, …) and override what they set. The whole block is one native view.

```tsx
<text style={{ fontSize: 17 }}>
  <text style={{ fontWeight: "300" }}>Light </text>
  <text style={{ fontWeight: "bold" }}>Bold </text>
  <text style={{ fontStyle: "italic", color: "#8250df" }}>Italic</text>
</text>
```

### Links

A span with a press handler is a link: a press on its characters goes to the span, a press elsewhere to the paragraph.

```tsx
<text style={{ fontSize: 15 }} onPress={() => log("the sentence")}>
  {"By continuing you agree to the "}
  <text style={{ color: "#0969da", textDecorationLine: "underline" }} onPress={() => open("terms")}>
    Terms of service
  </text>
  .
</text>
```

### Inline views and images

Any other element inside a `<text>` (a `<view>`, an `<image>`, even inside spans) is an inline attachment: it is laid out
at its own size, sits on the baseline and wraps with the words. It takes presses of its own.

```tsx
<text style={{ fontSize: 16, lineHeight: 26 }}>
  {"Made with "}
  <image src={logo} style={{ width: 18, height: 18, borderRadius: 4 }} />
  {" nsh. Status "}
  <view style={{ backgroundColor: "#1a7f37", borderRadius: 9, paddingHorizontal: 7, height: 18, justifyContent: "center" }}>
    <text style={{ color: "#ffffff", fontSize: 11, fontWeight: "700" }}>LIVE</text>
  </view>
</text>
```

Attachments cut off by `numberOfLines` or an ellipsis are hidden. Copying a selectable text leaves them out.

### Whitespace

JSX text follows React's rules: lines are trimmed except at the text's ends, whitespace-only lines are dropped and the
remaining lines are joined with one space. Spaces on one line are kept (`{a} {b}` keeps its space). Write
`{" "}` or a string expression when a span needs a leading or trailing space.

## Props

Besides the [common props](./index.md#common-props) (with `style` typed as `TextStyle`):

| Prop | Type | Default | Description |
|---|---|---|---|
| `numberOfLines` | `number` | `0` | The most lines shown; `0` is unlimited. |
| `ellipsizeMode` | `"tail" \| "head" \| "middle" \| "clip"` | `"tail"` | Where text that does not fit `numberOfLines` is cut. |
| `allowFontScaling` | `boolean` | `true` | Follow the system text size setting. |
| `maxFontSizeMultiplier` | `number` | `0` | The upper bound of the system scale; `0` is none. |
| `adjustsFontSizeToFit` | `boolean` | `false` | Shrink the font until the text fits its lines (`numberOfLines`) and its box. |
| `minimumFontScale` | `number` | | How far `adjustsFontSizeToFit` shrinks at most (0..1; at least 0.1). |
| `selectable` | `boolean` | `false` | A long press highlights the block and offers Copy. |
| `onTextLayout` | `(e: { lines: TextLayoutLine[] }) => void` | | The laid-out lines after each layout. |

The `select-text` Tailwind class is the same as `selectable`.

## Text styles

`TextStyle` adds to the view styles: `color`, `fontSize`, `fontWeight` (`"normal"`, `"bold"` or 100–900), `fontStyle`,
`fontFamily`, `lineHeight`, `letterSpacing`, `textAlign`, `textAlignVertical`, `textTransform`, `textDecorationLine`,
`textDecorationColor`, `textDecorationStyle`, `textDecorationThickness`, `textUnderlineOffset`, `textShadowColor`,
`textShadowOffset`, `textShadowRadius`, `fontVariant` and `writingDirection`. See the
[style reference](../modules/style-reference.md#textstyle) and [text](../docs/text.md).

`fontFamily` takes a system family or the name an imported font evaluates to:

```tsx
import Inter from "./fonts/Inter-Regular.ttf";

<text style={{ fontFamily: Inter, fontSize: 17 }}>Inter</text>;
```

The font is registered before the first frame. See [fonts](../docs/fonts.md).

## Events

| Event | Payload | When |
|---|---|---|
| `onTextLayout` | `{ lines: TextLayoutLine[] }` | After each layout of the paragraph. |
| `onPress`, `onLongPress`, … | none | As on [view](./view.md#events); on a span, presses on its characters. |
| `onLayout` | `{ x, y, width, height }` | The block's frame changed. |

`TextLayoutLine` is `{ text, x, y, width, height, ascender, descender, capHeight, xHeight }`, in the text's content box:

```tsx
const lines = signal(0);
<text onTextLayout={(e) => lines.set(e.lines.length)}>{article}</text>;
```

## Ref methods

None of its own.

## Platform notes

- `selectable` copies whole blocks: there is no range selection. iOS shows the edit menu, Android a floating action mode.
- Text decorations other than a solid single line (`double`, `dotted`, `dashed`, `wavy`) are drawn by nsh on iOS.
- iOS texts draw no gradient backgrounds or inset shadows.
- `fontScale()` from [`nsh/device`](../modules/nsh-device.md) reports the current system text scale.
