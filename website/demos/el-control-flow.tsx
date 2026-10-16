import { mount, signal } from "nsh";

interface Todo {
  id: number;
  title: string;
  done: boolean;
}

let nextId = 4;

function App() {
  const todos = signal<Todo[]>([
    { id: 1, title: "Read the elements page", done: true },
    { id: 2, title: "Try <if> and <each>", done: false },
    { id: 3, title: "Ship the app", done: false },
  ]);
  const showDone = signal(true);
  const visible = () => (showDone() ? todos() : todos().filter((t) => !t.done));
  const toggle = (id: number) => todos.update((list) => list.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));
  const add = () => todos.update((list) => [...list, { id: nextId++, title: `Task ${nextId - 1}`, done: false }]);
  const remove = (id: number) => todos.update((list) => list.filter((t) => t.id !== id));
  const button = { borderRadius: 8, paddingHorizontal: 12, paddingVertical: 8 } as const;

  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", padding: 20, paddingTop: 24, gap: 12 }}>
      <text style={{ fontSize: 22, fontWeight: "700", color: "#1f2328" }}>if / each</text>
      <view style={{ flexDirection: "row", gap: 8 }}>
        <view onPress={add} activeOpacity={0.6} style={{ ...button, backgroundColor: "#1f883d" }}>
          <text style={{ color: "#ffffff" }}>Add</text>
        </view>
        <view onPress={() => showDone.set(!showDone())} activeOpacity={0.6} style={{ ...button, backgroundColor: "#ddf4ff" }}>
          <text style={{ color: "#0969da" }}>{showDone() ? "Hide done" : "Show done"}</text>
        </view>
      </view>

      <if
        when={visible().length > 0}
        else={<text style={{ color: "#6e7781", textAlign: "center", marginTop: 40 }}>Nothing to show.</text>}
      >
        <view style={{ gap: 8 }}>
          <each of={visible()} key={(t: Todo) => t.id}>
            {(todo, index) => (
              <view
                style={{
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 12,
                  backgroundColor: "#ffffff",
                  borderRadius: 10,
                  padding: 12,
                  borderWidth: 1,
                  borderColor: "#d0d7de",
                }}
              >
                <view
                  onPress={() => toggle(todo().id)}
                  hitSlop={8}
                  style={{
                    width: 22,
                    height: 22,
                    borderRadius: 11,
                    borderWidth: 2,
                    borderColor: "#1f883d",
                    backgroundColor: todo().done ? "#1f883d" : "transparent",
                  }}
                />
                <text
                  style={{
                    flex: 1,
                    color: todo().done ? "#8c959f" : "#1f2328",
                    textDecorationLine: todo().done ? "line-through" : "none",
                  }}
                >
                  {index() + 1}. {todo().title}
                </text>
                <if when={todo().done}>
                  <view onPress={() => remove(todo().id)} hitSlop={8}>
                    <text style={{ color: "#cf222e" }}>Remove</text>
                  </view>
                </if>
              </view>
            )}
          </each>
        </view>
      </if>
      <text style={{ fontSize: 12, color: "#6e7781" }}>
        {todos().filter((t) => t.done).length} of {todos().length} done
      </text>
    </view>
  );
}

mount(App);
