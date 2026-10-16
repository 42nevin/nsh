---
title: JSX and components
description: How nsh compiles JSX, what a component is, how props, children, control flow, refs and spreads work, and how tags name elements and components.
order: 6
demo: guide-components
---

# JSX and components

nsh apps describe their UI with JSX, but there is no React and no virtual DOM. nsh's compiler turns each JSX element into direct calls of the native renderer, and a component is a plain function that runs once to build its part of the tree. What keeps the UI up to date afterwards is [reactivity](./reactivity.md): expressions in JSX that read signals are followed natively, and only they run again.

## Elements

Lowercase tags are built-in **host elements**, each backed by a native view:

| Tag | Native view | Page |
|---|---|---|
| `<view>` | `UIView` / `ViewGroup`: a box for layout, styling and touches | [view](../elements/view.md) |
| `<text>` | A native text view; nested `<text>` elements are spans | [text](../elements/text.md) |
| `<image>` | An image view (local assets and URLs) | [image](../elements/image.md) |
| `<scroll>` | `UIScrollView` / `ScrollView` | [scroll](../elements/scroll.md) |
| `<input>` | `UITextField` / `UITextView` / `EditText` | [input](../elements/input.md) |
| `<list>` | A virtualized, recycling list (also `List` from `nsh/list`) | [list](../elements/list.md) |
| `<canvas>`, `<web-view>`, `<video>`, `<modal>` | Typed through their modules' components (`Canvas`, `WebView`, ...) | [elements](../elements/index.md) |
| `<if>`, `<each>` | Control flow, no view of its own | [control flow](../elements/control-flow.md) |

Props keep their camelCase names (`onPress`, `numberOfLines`, `backgroundColor`). Every element accepts `style`, `class` / `classList` ([Tailwind](./tailwind.md)), `ref`, touch handlers, accessibility props and `testID`; platform-only props go in `ios={{…}}` and `android={{…}}`, and only the current platform's object is sent:

```tsx
<view android={{ elevation: 4, ripple: { color: "#0001" } }} style={{ borderRadius: 12 }} />
```

An unknown lowercase tag is a build error that lists the built-ins:

```text
x unknown element <foo-bar> (built-in elements: view, text, image, scroll,
| input, canvas, web-view, video, modal, input-accessory; `if`, `each`; or a
| component in scope, by its name in kebab case)
```

> [!IMPORTANT]
> Text is only displayed inside `<text>`. A string directly in a `<view>` creates no native view and is not shown.

## Components

A component is a function that takes a props object and returns JSX:

```tsx
function Badge(props: { label: string; color?: string }) {
  return (
    <view style={{ borderRadius: 999, paddingHorizontal: 10, paddingVertical: 4, backgroundColor: props.color ?? "#ddf4ff" }}>
      <text style={{ fontSize: 13, fontWeight: "600" }}>{props.label}</text>
    </view>
  );
}

<Badge label={`${unread()} new`} color="#fff8c5" />
```

The function runs **once**, when the component is created, and does not track what it reads: there is no re-rendering. Code in its body (creating signals, starting effects, computing constants) runs one time. The JSX it returns stays connected to the props and signals it reads, so `Badge` above updates its text whenever `unread()` changes, without running `Badge` again.

A component may return an element, a string or number, an array, `null`, or a function returning any of these (which the renderer follows). The `Component<P>` type from `nsh` describes one:

```tsx
import type { Component, JSX } from "nsh";

const Row: Component<{ title: string; children?: JSX.Element }> = (props) => (
  <view style={{ flexDirection: "row", gap: 8 }}>
    <text>{props.title}</text>
    {props.children}
  </view>
);
```

### What runs again

The compiler decides for each expression in JSX whether it can change. An expression changes when it **calls a function or reads a property** (outside nested functions); everything else is set once:

```tsx
<text>{count()}</text>              // follows count
<text>{props.title}</text>          // follows the prop
<text>{user.name}</text>            // re-read when something it depends on changes
<text>{label}</text>                // a plain variable: set once
<view style={{ opacity: busy() ? 0.5 : 1 }} />   // the style object is rebuilt when busy changes
```

A changing prop or child is a small native effect that sets that one prop or replaces that one child. Style objects are diffed in Rust, so rebuilding a style only sends the properties that changed.

Event handlers are set once when they are a function or a name (`onPress={() => …}`, `onPress={save}`), and followed otherwise (`onPress={props.onPress}` follows the prop).

### Props are getters

A prop given a changing expression is passed as a getter: reading `props.title` calls it, which is what lets the JSX that reads it follow it. Static props are plain values. This is why props must be read where they are used:

```tsx
// Wrong: reads the prop once, when the component runs.
function Title(props: { text: string }) {
  const { text } = props;
  return <text>{text}</text>;
}

// Right: reads it in JSX, which follows it.
function Title(props: { text: string }) {
  return <text>{props.text}</text>;
}
```

The same applies to default values: write `props.color ?? "#ddf4ff"` in the JSX rather than destructuring with a default. To compute something from props, use a function or [`computed`](./reactivity.md#computed):

```tsx
function Price(props: { cents: number }) {
  const formatted = () => `$${(props.cents / 100).toFixed(2)}`;
  return <text>{formatted()}</text>;
}
```

### Children

What you put between a component's tags arrives as `props.children`, a getter that builds those elements when read:

```tsx
function Card(props: { title: string; children?: JSX.Element }) {
  return (
    <view style={{ padding: 14, borderRadius: 12, backgroundColor: "#ffffff", gap: 8 }}>
      <text style={{ fontWeight: "600" }}>{props.title}</text>
      {props.children}
    </view>
  );
}

<Card title="Account">
  <text>Signed in as {user().name}</text>
</Card>
```

Each read of `props.children` builds the children again (new native views). Read it once, in the place where the children go. Children can also be a function, a render callback the component calls with arguments:

```tsx
function Repeat(props: { times: number; children: (i: number) => JSX.Element }) {
  return <view>{Array.from({ length: props.times }, (_, i) => props.children(i))}</view>;
}

<Repeat times={3}>{(i) => <text>Row {i}</text>}</Repeat>
```

Fragments (`<>…</>`) group elements without a view.

## Control flow

`<if>` and `<each>` are built-in control flow. They render no view of their own.

```tsx
<if when={user()} else={<text>Signed out</text>}>
  <text>Hello, {user()!.name}</text>
</if>

<each of={todos()} key={(t) => t.id}>
  {(todo, index) => (
    <text>
      {index() + 1}. {todo().title}
    </text>
  )}
</each>
```

- **`<if when else>`** renders its children while `when` is truthy, else the `else` element. It renders a branch again only when the condition's truthiness changes, not every time `when` is re-evaluated.
- **`<each of key>`** renders its only child, a function, once per item. Rows are kept by `key(item)` (default: the item itself; objects by identity, primitives by value): when the array changes, kept rows are moved rather than rebuilt, and new rows are made only for new keys. The function receives **accessors**, `todo()` and `index()`, because a kept row's item and position can change.

A ternary or `&&` in JSX also works (`{busy() ? <Spinner /> : <Content />}`), but it builds its branch again every time anything it reads changes. Prefer `<if>` when a branch is more than a few elements.

For long or infinite lists, use [`List`](./lists.md): it creates views only for the rows on screen and recycles them. See [Control flow](../elements/control-flow.md) for the details.

## Refs

`ref` gives you the native node of a host element, a `YNode`, for imperative calls: `focus()`, `blur()`, `scrollTo()`, `scrollToIndex()`, ... A variable is assigned, a function is called:

```tsx
import type { YNode } from "nsh";

function Search() {
  let input: YNode | undefined;
  return (
    <view>
      <input ref={input} placeholder="Search" />
      <view onPress={() => input?.focus()}>
        <text>Focus</text>
      </view>
    </view>
  );
}

<scroll ref={(node) => (scroller = node)} />
```

The ref is set when the element is created, before the component returns, so it is available in [effects](./reactivity.md#effect) and event handlers. Calls are sent to the view in order with the changes made before them. Components from `nsh/ui/mobile` accept `ref` too.

## Spreading props

Spread an object of props onto an element or a component with `{...props}`. Getters stay getters, so spread props keep following what they read. To take some props for yourself and pass the rest on, use `split`:

```tsx
import { type InputProps, split } from "nsh";

function Field(props: InputProps & { label: string }) {
  const [own, rest] = split(props, ["label"]);
  return (
    <view style={{ gap: 4 }}>
      <text>{own.label}</text>
      <input style={{ borderWidth: 1, borderRadius: 8, padding: 8 }} {...rest} />
    </view>
  );
}

<Field label="Email" value={email()} onChangeText={(t) => email.set(t)} keyboardType="email-address" />
```

`split(props, keys)` returns `[picked, rest]`, both still reading through to the original props (destructuring `{ label, ...rest } = props` would read every prop once). Element props that come later in the JSX win over earlier ones.

`<if>` and `<each>` take no spread props, and spread children (`{...items}`) are not supported: use `<each>` or an array expression.

## Tags and names

A capitalized tag names a binding in scope, as usual: `<Badge />`, `<Theme.Provider>`. A component can also be written in **kebab case**: `WeatherPanel` is also `<weather-panel>`, and an imported `Button` is also `<button>`.

```tsx
import { Button } from "nsh/ui/mobile";

<button title="Save" onPress={save} />    // the Button component
```

Your bindings win over built-ins of the same name: a component named `Text` in scope makes `<text>` that component, and an imported `Modal` makes `<modal>` it. Name your components so they do not collide with the element names unless that is what you want. (Objects of functions imported from nsh's modules are not components: importing `Image` from `nsh/image` leaves `<image>` the element.)

Exported or imported components used in kebab case are typed in `.nsh/types/nsh/jsx-runtime.d.ts` after the next build, so the editor checks their props.

## Mounting and the root

`mount(App)` renders the root component; `mount(App, props)` passes it props. The root view fills the surface (the window, or the view an app embeds nsh in). Give the root `flex: 1` so it fills the screen, and a background color: the surface's own background depends on the platform.

```tsx
mount(() => (
  <view style={{ flex: 1, backgroundColor: "#ffffff" }}>
    <App />
  </view>
));
```

## Related

- [Reactivity](./reactivity.md): signals, effects, context.
- [Elements](../elements/index.md): every host element and its props.
- [Components](../components/index.md): the ready-made components of `nsh/ui/mobile`.
