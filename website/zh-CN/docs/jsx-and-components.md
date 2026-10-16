---
title: JSX 与组件
description: nsh 如何编译 JSX、组件是什么，props、children、控制流、ref 和展开如何工作，以及标签如何对应元素与组件。
order: 6
demo: guide-components
---

# JSX 与组件

nsh 应用用 JSX 描述界面，但其中没有 React，也没有虚拟 DOM。nsh 的编译器把每个 JSX 元素转换成对原生渲染器的直接调用，而组件只是一个普通函数，运行一次来构建它负责的那部分树。之后让界面保持最新的是[响应式](./reactivity.md)：JSX 中读取 signal 的表达式由原生层跟踪，变化时只有它们会重新运行。

## 元素

小写标签是内置的**宿主元素**，每个都对应一个原生视图：

| 标签 | 原生视图 | 页面 |
|---|---|---|
| `<view>` | `UIView` / `ViewGroup`：用于布局、样式和触摸的盒子 | [view](../elements/view.md) |
| `<text>` | 原生文本视图；嵌套的 `<text>` 元素是其中的片段（span） | [text](../elements/text.md) |
| `<image>` | 图片视图（本地资源和 URL） | [image](../elements/image.md) |
| `<scroll>` | `UIScrollView` / `ScrollView` | [scroll](../elements/scroll.md) |
| `<input>` | `UITextField` / `UITextView` / `EditText` | [input](../elements/input.md) |
| `<list>` | 虚拟化、可复用单元格的列表（也可用 `nsh/list` 中的 `List`） | [list](../elements/list.md) |
| `<canvas>`、`<web-view>`、`<video>`、`<modal>` | 通过各自模块中的组件获得类型（`Canvas`、`WebView` 等） | [元素](../elements/index.md) |
| `<if>`、`<each>` | 控制流，本身不产生视图 | [控制流](../elements/control-flow.md) |

props 保留 camelCase 命名（`onPress`、`numberOfLines`、`backgroundColor`）。每个元素都接受 `style`、`class` / `classList`（[Tailwind](./tailwind.md)）、`ref`、触摸处理函数、无障碍属性和 `testID`；仅限某个平台的 props 放在 `ios={{…}}` 和 `android={{…}}` 中，只有当前平台的对象会被发送：

```tsx
<view android={{ elevation: 4, ripple: { color: "#0001" } }} style={{ borderRadius: 12 }} />
```

未知的小写标签会导致构建错误，错误信息会列出所有内置元素：

```text
x unknown element <foo-bar> (built-in elements: view, text, image, scroll,
| input, canvas, web-view, video, modal, input-accessory; `if`, `each`; or a
| component in scope, by its name in kebab case)
```

> [!IMPORTANT]
> 文本只有放在 `<text>` 中才会显示。直接写在 `<view>` 里的字符串不会创建原生视图，也不会显示。

## 组件

组件是一个接收 props 对象并返回 JSX 的函数：

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

这个函数只在组件创建时运行**一次**，而且不会跟踪它读取了什么：不存在重新渲染。函数体中的代码（创建 signal、启动 effect、计算常量）只执行一次。它返回的 JSX 与其读取的 props 和 signal 保持连接，所以上面的 `Badge` 会在 `unread()` 变化时更新文本，而无需再次运行 `Badge`。

组件可以返回一个元素、字符串或数字、数组、`null`，或者返回上述任意值的函数（渲染器会跟踪它）。`nsh` 中的 `Component<P>` 类型描述的就是组件：

```tsx
import type { Component, JSX } from "nsh";

const Row: Component<{ title: string; children?: JSX.Element }> = (props) => (
  <view style={{ flexDirection: "row", gap: 8 }}>
    <text>{props.title}</text>
    {props.children}
  </view>
);
```

### 哪些会重新运行

编译器会为 JSX 中的每个表达式判断它是否可能变化。表达式只要**调用函数或读取属性**（嵌套函数之外），就被视为会变化；其余的只设置一次：

```tsx
<text>{count()}</text>              // follows count
<text>{props.title}</text>          // follows the prop
<text>{user.name}</text>            // re-read when something it depends on changes
<text>{label}</text>                // a plain variable: set once
<view style={{ opacity: busy() ? 0.5 : 1 }} />   // the style object is rebuilt when busy changes
```

会变化的 prop 或子节点对应一个很小的原生 effect，只设置那一个 prop 或替换那一个子节点。样式对象在 Rust 中做 diff，所以重建样式时只会发送真正变化的属性。

事件处理函数如果是函数字面量或一个名字（`onPress={() => …}`、`onPress={save}`），只设置一次；否则会被跟踪（`onPress={props.onPress}` 会跟随这个 prop）。

### props 是 getter

传入会变化的表达式的 prop 以 getter 的形式传递：读取 `props.title` 就会调用它，正因如此，读取它的 JSX 才能跟随变化。静态 props 是普通值。这就是为什么 props 必须在使用的地方读取：

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

默认值也是一样：在 JSX 中写 `props.color ?? "#ddf4ff"`，而不是用带默认值的解构。要从 props 计算出某个值，使用一个函数或 [`computed`](./reactivity.md#computed)：

```tsx
function Price(props: { cents: number }) {
  const formatted = () => `$${(props.cents / 100).toFixed(2)}`;
  return <text>{formatted()}</text>;
}
```

### children

写在组件标签之间的内容会作为 `props.children` 传入，它是一个 getter，被读取时才构建这些元素：

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

每读取一次 `props.children` 都会重新构建一遍子元素（新的原生视图）。只在子元素应该出现的位置读取一次。children 也可以是函数，即组件带参数调用的渲染回调：

```tsx
function Repeat(props: { times: number; children: (i: number) => JSX.Element }) {
  return <view>{Array.from({ length: props.times }, (_, i) => props.children(i))}</view>;
}

<Repeat times={3}>{(i) => <text>Row {i}</text>}</Repeat>
```

Fragment（`<>…</>`）可以把多个元素组合在一起而不产生视图。

## 控制流

`<if>` 和 `<each>` 是内置的控制流，本身不渲染视图。

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

- **`<if when else>`** 在 `when` 为真值时渲染子元素，否则渲染 `else` 元素。只有当条件的真假发生变化时才会重新渲染分支，而不是每次重新计算 `when` 都渲染。
- **`<each of key>`** 为每一项渲染它唯一的子节点（一个函数）。行按 `key(item)` 保留（默认是项本身：对象按引用，原始值按值）：数组变化时，保留下来的行会被移动而不是重建，只有新的 key 才会创建新行。函数接收的是**访问器** `todo()` 和 `index()`，因为保留下来的行，其对应的项和位置都可能改变。

在 JSX 中使用三元表达式或 `&&` 也可以（`{busy() ? <Spinner /> : <Content />}`），但只要它读取的任何值变化，就会重建分支。分支包含较多元素时，优先使用 `<if>`。

对于很长或无限的列表，使用 [`List`](./lists.md)：它只为屏幕上的行创建视图，并复用这些视图。详见[控制流](../elements/control-flow.md)。

## ref

`ref` 让你拿到宿主元素的原生节点（一个 `YNode`），用于命令式调用：`focus()`、`blur()`、`scrollTo()`、`scrollToIndex()` 等。传入变量时会给它赋值，传入函数时会调用它：

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

ref 在元素创建时就被设置，早于组件返回，因此在 [effect](./reactivity.md#effect) 和事件处理函数中都可以使用。调用会按顺序发送给视图，排在它之前的修改之后。`nsh/ui/mobile` 中的组件同样接受 `ref`。

## 展开 props

用 `{...props}` 把一个 props 对象展开到元素或组件上。getter 仍然是 getter，所以展开的 props 会继续跟随它们读取的值。如果要自己取走一部分 props、把其余的继续传下去，使用 `split`：

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

`split(props, keys)` 返回 `[picked, rest]`，两者都仍然读取原始 props（而解构 `{ label, ...rest } = props` 会把每个 prop 都读取一次）。在 JSX 中靠后的元素 props 会覆盖靠前的。

`<if>` 和 `<each>` 不接受展开 props，也不支持展开子节点（`{...items}`）：请使用 `<each>` 或数组表达式。

## 标签与名称

大写开头的标签和平常一样，指向作用域中的绑定：`<Badge />`、`<Theme.Provider>`。组件也可以用 **kebab-case** 书写：`WeatherPanel` 也可以写成 `<weather-panel>`，导入的 `Button` 也可以写成 `<button>`。

```tsx
import { Button } from "nsh/ui/mobile";

<button title="Save" onPress={save} />    // the Button component
```

你的绑定优先于同名的内置元素：作用域中名为 `Text` 的组件会让 `<text>` 指向这个组件，导入的 `Modal` 也会让 `<modal>` 指向它。除非你正是想这样，否则组件命名应避免与元素名冲突。（从 nsh 模块导入的函数对象不是组件：从 `nsh/image` 导入 `Image` 后，`<image>` 仍然是元素。）

以 kebab-case 使用的导出或导入组件，会在下一次构建后在 `.nsh/types/nsh/jsx-runtime.d.ts` 中获得类型，编辑器就能检查它们的 props。

## 挂载与根视图

`mount(App)` 渲染根组件；`mount(App, props)` 同时向它传入 props。根视图填满整个 surface（窗口，或应用嵌入 nsh 时所用的视图）。给根视图设置 `flex: 1` 让它铺满屏幕，并设置背景色：surface 本身的背景因平台而异。

```tsx
mount(() => (
  <view style={{ flex: 1, backgroundColor: "#ffffff" }}>
    <App />
  </view>
));
```

## 相关内容

- [响应式](./reactivity.md)：signal、effect、context。
- [元素](../elements/index.md)：所有宿主元素及其 props。
- [组件](../components/index.md)：`nsh/ui/mobile` 中现成的组件。
