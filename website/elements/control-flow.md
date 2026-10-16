---
title: if / each
description: "The built-in control flow tags: <if> renders a branch by a condition, <each> renders a keyed row per item of an array."
order: 12
demo: el-control-flow
---

# if / each

`<if>` and `<each>` are built-in tags for conditional and repeated content. They create no native view; they decide which
elements exist. Both are compiled into calls to the native renderer, which updates only what changed: `<if>` swaps a
branch when the condition's truthiness changes, `<each>` adds, moves and removes rows by key instead of rebuilding the
list.

```tsx
<view>
  <if when={user()} else={<text>Signed out</text>}>
    <text>Hello, {user()!.name}</text>
  </if>
  <each of={todos()}>{(todo) => <text>{todo().title}</text>}</each>
</view>
```

They need no import. Inside JSX expressions you can still use plain JavaScript (`cond ? a : b`, `items.map(…)`), but those
rebuild their elements every time what they read changes; `<if>` and `<each>` keep the elements that stay.

## `<if>`

Renders its children while `when` is truthy, else the `else` element (or nothing).

| Prop | Type | Description |
|---|---|---|
| `when` | `unknown` | The condition. Only its truthiness matters. |
| `else` | `JSX.Element` | Rendered while `when` is falsy. |
| `children` | `JSX.Element` | Rendered while `when` is truthy. |

The branch is created again only when the truthiness of `when` changes. A `when` that goes from one truthy value to
another (`user()` changing from one object to another) keeps the branch, and the expressions inside it update on their
own.

```tsx
const loading = signal(true);

<if when={!loading()} else={<text style={{ color: "#6e7781" }}>Loading…</text>}>
  <Profile />
</if>;
```

## `<each>`

Renders `children(item, index)` for each item of `of`.

| Prop | Type | Description |
|---|---|---|
| `of` | `readonly any[] \| null \| undefined` | The items. `null` and `undefined` render nothing. |
| `key` | `(item: any) => unknown` | The identity of an item. Default: the item itself (objects by identity, primitives by value). |
| `children` | `(item: () => any, index: () => number) => JSX.Element` | Renders one row. |

The row function gets **accessors**, not values: call `item()` and `index()`. When `of` changes, rows whose key is still
there are kept (and moved if their position changed); their `item()` and `index()` update, and only the expressions
reading them run again. New keys get new rows; rows whose key is gone are removed.

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

### Choosing a key

- Items replaced by new objects on every update (immutable updates, `{ ...t, done: true }`) need a `key`, usually an id.
  Without it each new object is a new row and its elements are created again.
- Arrays of strings or numbers work without a key: they are compared by value.
- Each row has its own owner: effects and cleanups created in a row end when the row is removed.

> [!NOTE]
> The `<each>` tag is not generic, so TypeScript cannot infer the item type from `of`. Annotate the `key` parameter, or
> cast in the row (`todo() as Todo`).

## `<each>` or a list

`<each>` creates every row. That is right for tens or a few hundred items, in a `<view>` or a [`<scroll>`](./scroll.md).
For long or unbounded data, use a [list](./list.md): it creates only the rows near the viewport and recycles them.

See [JSX and components](../docs/jsx-and-components.md) and [reactivity](../docs/reactivity.md) for how expressions
update.
