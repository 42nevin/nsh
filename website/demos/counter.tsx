import { mount, signal } from "nsh";

function Button(props: { label: string; color: string; onPress: () => void }) {
  return (
    <view
      onPress={props.onPress}
      style={{
        backgroundColor: props.color,
        borderRadius: 12,
        paddingVertical: 12,
        paddingHorizontal: 24,
        alignItems: "center",
      }}
    >
      <text style={{ color: "#ffffff", fontSize: 18, fontWeight: "bold" }}>{props.label}</text>
    </view>
  );
}

function App() {
  const count = signal(0);
  const history = signal<number[]>([]);

  const change = (delta: number) => {
    const next = count() + delta;
    count.set(next);
    history.update((h) => [next, ...h].slice(0, 5));
  };

  return (
    <view style={{ flex: 1, backgroundColor: "#f4f5f7", padding: 24, paddingTop: 64, gap: 24 }}>
      <text style={{ fontSize: 28, fontWeight: "bold", color: "#1f2328" }}>nsh counter</text>
      <view
        style={{
          backgroundColor: "#ffffff",
          borderRadius: 16,
          padding: 24,
          alignItems: "center",
          gap: 8,
        }}
      >
        <text style={{ fontSize: 14, color: "#6e7781" }}>Current value</text>
        <text style={{ fontSize: 64, fontWeight: "bold", color: count() < 0 ? "#cf222e" : "#1f2328" }}>
          {count()}
        </text>
      </view>
      <view style={{ flexDirection: "row", gap: 12, justifyContent: "center" }}>
        <Button label="−1" color="#cf222e" onPress={() => change(-1)} />
        <Button label="+1" color="#1f883d" onPress={() => change(1)} />
      </view>
      <view style={{ gap: 6 }}>
        <text style={{ fontSize: 14, color: "#6e7781" }}>
          Recent values: <text style={{ fontWeight: "bold", color: "#1f2328" }}>{history().length}</text>
        </text>
        <each of={history()}>{(value) => <text style={{ fontSize: 16, color: "#1f2328" }}>• {value()}</text>}</each>
      </view>
    </view>
  );
}

mount(App);
