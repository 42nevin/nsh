---
title: if / each
description: 内置的控制流标签：<if> 按条件渲染一个分支，<each> 为数组的每一项渲染一个带 key 的行。
order: 12
demo: el-control-flow
---

# if / each

`<if>` 和 `<each>` 是用于条件内容和重复内容的内置标签。它们不创建原生视图，只决定哪些元素存在。两者都会被编译成对原生渲染器的调用，只更新发生变化的部分：`<if>` 在条件的真假发生变化时切换分支，`<each>` 按 key 增加、移动和删除行，而不是重建整个列表。

```tsx
<view>
  <if when={user()} else={<text>Signed out</text>}>
    <text>Hello, {user()!.name}</text>
  </if>
  <each of={todos()}>{(todo) => <text>{todo().title}</text>}</each>
</view>
```

它们无需导入。在 JSX 表达式里仍然可以使用普通 JavaScript（`cond ? a : b`、`items.map(…)`），但这些写法会在其读取的值变化时重建元素；`<if>` 和 `<each>` 则会保留不变的元素。

## `<if>`

`when` 为真值时渲染子元素，否则渲染 `else` 元素（或什么都不渲染）。

| 属性 | 类型 | 说明 |
|---|---|---|
| `when` | `unknown` | 条件。只看它的真假。 |
| `else` | `JSX.Element` | `when` 为假值时渲染。 |
| `children` | `JSX.Element` | `when` 为真值时渲染。 |

只有 `when` 的真假变化时才会重新创建分支。如果 `when` 从一个真值变成另一个真值（例如 `user()` 从一个对象变成另一个对象），分支会保留，其中的表达式各自更新。

```tsx
const loading = signal(true);

<if when={!loading()} else={<text style={{ color: "#6e7781" }}>Loading…</text>}>
  <Profile />
</if>;
```

## `<each>`

为 `of` 的每一项渲染 `children(item, index)`。

| 属性 | 类型 | 说明 |
|---|---|---|
| `of` | `readonly any[] \| null \| undefined` | 条目。`null` 和 `undefined` 不渲染任何内容。 |
| `key` | `(item: any) => unknown` | 条目的标识。默认是条目本身（对象按引用比较，原始值按值比较）。 |
| `children` | `(item: () => any, index: () => number) => JSX.Element` | 渲染一行。 |

行函数拿到的是**访问器**而不是值：要调用 `item()` 和 `index()`。`of` 变化时，key 仍然存在的行会被保留（位置变了就移动），它们的 `item()` 和 `index()` 会更新，只有读取它们的表达式会重新执行。新的 key 产生新行；key 消失的行被删除。

```tsx
interface Todo {
  id: number;
  title: string;
  done: boolean;
}

const todos = signal<Todo[]>([]);

<each of={todos()} key={(t: Todo) => t.id}>
  {(todo, index) => (
    <view style={{ flexDirection: "row", gap: 8 }}>
      <text>{index() + 1}.</text>
      <text style={{ textDecorationLine: todo().done ? "line-through" : "none" }}>{todo().title}</text>
    </view>
  )}
</each>;
```

### 如何选择 key

- 如果每次更新都用新对象替换条目（不可变更新，`{ ...t, done: true }`），就需要 `key`，通常是 id。否则每个新对象都是一个新行，其中的元素会被重新创建。
- 字符串或数字数组不需要 key：它们按值比较。
- 每一行都有自己的 owner：在行中创建的 effect 和清理函数会在行被删除时结束。

> [!NOTE]
> `<each>` 标签不是泛型的，TypeScript 无法从 `of` 推断条目类型。请给 `key` 的参数加上类型注解，或在行中做类型断言（`todo() as Todo`）。

## `<each>` 还是列表

`<each>` 会创建每一行。对于几十到几百个条目，放在 `<view>` 或 [`<scroll>`](./scroll.md) 里正合适。对于很长或无上限的数据，请使用[列表](./list.md)：它只创建视口附近的行并复用它们。

表达式如何更新，参见 [JSX 与组件](../docs/jsx-and-components.md) 和[响应式](../docs/reactivity.md)。
