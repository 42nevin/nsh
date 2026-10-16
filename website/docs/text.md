---
title: Text
description: Showing text with the text element - nested spans and links, inline views, fonts and decorations, truncation, system text size, selection and onTextLayout.
order: 11
---

# Text

All text is shown by `<text>`. A `<text>` element is one paragraph, typeset by the platform's text engine (CoreText on Apple platforms, `StaticLayout` on Android) on the engine thread when it is laid out, and only drawn on the main thread. Nested `<text>` elements are spans of the same paragraph, not separate views.

```tsx
<text style={{ fontSize: 16, lineHeight: 24, color: "#1f2328" }}>
  By continuing you agree to the{" "}
  <text style={{ color: "#0969da", textDecorationLine: "underline" }} onPress={openTerms}>
    Terms of service
  </text>
  .
</text>
```

> [!IMPORTANT]
> Strings are only displayed inside `<text>`. A string placed directly in a `<view>` is not shown.

The element itself is documented on [text](../elements/text.md); this page is about using it.

## Strings and whitespace

JSX text follows the same rules as in React: each line is trimmed, lines that are only whitespace are dropped, and the remaining lines are joined with a single space. Whitespace within one line is kept, so `{first} {last}` keeps its space. Use `{" "}` for a space at the start or end of a line, and `{"\n"}` for a line break:

```tsx
<text>
  Hello,
  {name()}!{"\n"}
  Welcome back.
</text>
```

Expressions in text follow what they read: `<text>{count()} items</text>` updates its first span when `count` changes.

## Spans and inheritance

A `<text>` inside a `<text>` is a span. It inherits the text style of its parent and overrides what it sets:

```tsx
<text style={{ fontSize: 17, color: "#1f2328" }}>
  <text style={{ fontWeight: "300" }}>Light </text>
  <text style={{ fontWeight: "700" }}>Bold </text>
  <text style={{ fontStyle: "italic" }}>Italic </text>
  <text style={{ letterSpacing: 4 }}>Spaced</text>
</text>
```

Text style classes also pass down from views, as in CSS: `<view class="text-white text-sm">` styles the `<text>` and `<input>` elements inside it.

### Links

A span with `onPress` (or `onLongPress`) is a link: a press on its characters goes to the span, a press elsewhere to the paragraph. Links in nested spans work too.

```tsx
<text onPress={() => log("the sentence")}>
  Read the <text style={{ color: "#0969da" }} onPress={() => open(termsUrl)}>terms</text> first.
</text>
```

### Views and images inside text

Any element other than `<text>` inside a `<text>` is laid out inline, on the baseline, and wraps with the words. It can take presses of its own:

```tsx
<text style={{ fontSize: 16, lineHeight: 26 }}>
  Made with <image src={logo} style={{ width: 18, height: 18 }} /> nsh. Status{" "}
  <view style={{ backgroundColor: "#1a7f37", borderRadius: 9, paddingHorizontal: 7, height: 18, justifyContent: "center" }}>
    <text style={{ color: "#ffffff", fontSize: 11, fontWeight: "700" }}>LIVE</text>
  </view>
</text>
```

Inline elements cut off by `numberOfLines` or an ellipsis are hidden; copying the text leaves them out.

## Text styles

| Property | Values |
|---|---|
| `color` | A [color](./styling.md#colors). Default black |
| `fontSize` | Points. Default 14 |
| `fontWeight` | `"normal"` (400), `"bold"` (700), or `100`–`900` (number or string) |
| `fontStyle` | `"normal"`, `"italic"` |
| `fontFamily` | A system family, a generic family, or an [imported font](./fonts.md) |
| `lineHeight` | Points |
| `letterSpacing` | Points |
| `textAlign` | `"auto"`, `"left"`, `"center"`, `"right"`, `"justify"` |
| `textAlignVertical` | `"auto"`, `"top"`, `"center"`, `"bottom"`: where text shorter than its box sits |
| `textTransform` | `"none"`, `"uppercase"`, `"lowercase"`, `"capitalize"` |
| `textDecorationLine` | `"none"`, `"underline"`, `"line-through"`, `"underline line-through"` |
| `textDecorationColor`, `textDecorationStyle` | Color; `"solid"`, `"double"`, `"dotted"`, `"dashed"`, `"wavy"` |
| `textDecorationThickness`, `textUnderlineOffset` | Points, or `"auto"` (thickness also `"from-font"`) |
| `textShadowColor`, `textShadowOffset`, `textShadowRadius` | A shadow behind the glyphs |
| `fontVariant` | OpenType features: `["tabular-nums"]`, `"small-caps"`, `"oldstyle-nums"`, `"lining-nums"`, `"proportional-nums"` |
| `writingDirection` | `"auto"`, `"ltr"`, `"rtl"`: the base direction of the paragraph |

A `<text>` is also a view: it takes backgrounds, borders, corners, padding and the other [view styles](./styling.md).

```tsx
<text style={{ fontSize: 18, fontVariant: ["tabular-nums"] }}>{price().toFixed(2)}</text>
<text style={{ textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationColor: "#cf222e" }}>Typo</text>
<text style={{ fontSize: 26, fontWeight: "800", color: "#ffffff", textShadowColor: "rgba(0,0,0,0.6)", textShadowOffset: { width: 2, height: 2 }, textShadowRadius: 4 }}>
  Shadow
</text>
```

`tabular-nums` gives every digit the same width, which keeps counters and prices from jittering as they change.

## Truncation

```tsx
<text numberOfLines={2}>{longDescription}</text>
<text numberOfLines={1} ellipsizeMode="middle">{filePath}</text>
```

| Prop | Description |
|---|---|
| `numberOfLines` | The most lines shown; `0` (default) is unlimited |
| `ellipsizeMode` | Where text that does not fit is cut: `"tail"` (default), `"head"`, `"middle"`, `"clip"` |
| `adjustsFontSizeToFit` | Shrink the font until the text fits its lines and box |
| `minimumFontScale` | How far `adjustsFontSizeToFit` may shrink (0–1) |

```tsx
<text numberOfLines={1} adjustsFontSizeToFit minimumFontScale={0.5} style={{ fontSize: 48, width: 200 }}>
  {total()}
</text>
```

The Tailwind classes `truncate` and `line-clamp-*` do the same and lose to the props when both are set.

## System text size

Text follows the user's text size setting (Dynamic Type on iOS, font size on Android) by default.

| Prop | Description |
|---|---|
| `allowFontScaling` | `false` keeps the size fixed. Default `true` |
| `maxFontSizeMultiplier` | The largest scale applied, e.g. `1.5`. `0` is no limit |

`fontScale()` from [`nsh/device`](../modules/nsh-device.md) is the current scale, an accessor that changes when the user changes the setting. `<input>` takes `allowFontScaling` and `maxFontSizeMultiplier` too.

```tsx
<text maxFontSizeMultiplier={1.3} style={{ fontSize: 13 }}>Tab label</text>
```

Prefer limiting the scale over turning it off: large text is an accessibility setting people rely on.

## Selection

`selectable` lets the user copy the text: a long press highlights the whole paragraph and offers Copy (the iOS edit menu, an Android floating action mode). Selection covers whole paragraphs, not ranges. The class `select-text` does the same.

```tsx
<text selectable>{orderNumber}</text>
```

## onTextLayout

`onTextLayout` reports the laid-out lines after each layout, in the text's content box:

```tsx
<text
  onTextLayout={(e) => {
    lines.set(e.lines.length);
    console.log(e.lines.map((l) => `${l.text} (${l.width.toFixed(0)}pt)`));
  }}
>
  {body()}
</text>
```

Each line has `text`, `x`, `y`, `width`, `height`, `ascender`, `descender`, `capHeight` and `xHeight`. Use it to show a "Read more" button only when text was cut, or to align other content with a baseline.

## Accessibility

A `<text>` is read by screen readers as its content. Give a role to text that acts as a heading (`accessibilityRole="header"`) and to links (`accessibilityRole="link"`). See [Accessibility](./accessibility.md).

## Related

- [Fonts](./fonts.md): custom fonts and weights.
- [text element](../elements/text.md): every prop.
- [Forms and keyboard](./forms-and-keyboard.md): text input.
