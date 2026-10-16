import { mount, signal } from "nsh";

type Direction = "column" | "row";
type Justify = "flex-start" | "center" | "flex-end" | "space-between" | "space-evenly";
type Align = "stretch" | "flex-start" | "center" | "flex-end";

function Choice<T extends string>(props: { label: string; options: readonly T[]; value: T; onChange: (v: T) => void }) {
  return (
    <view style={{ gap: 4 }}>
      <text style={{ fontSize: 12, fontWeight: "600", color: "#6e7781" }}>{props.label}</text>
      <view style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <each of={props.options}>
          {(o) => (
            <view
              onPress={() => props.onChange(o())}
              style={{ paddingHorizontal: 10, paddingVertical: 5, borderRadius: 6, backgroundColor: props.value === o() ? "#0969da" : "#ffffff" }}
            >
              <text style={{ fontSize: 12, color: props.value === o() ? "#ffffff" : "#1f2328" }}>{o()}</text>
            </view>
          )}
        </each>
      </view>
    </view>
  );
}

function App() {
  const direction = signal<Direction>("row");
  const justify = signal<Justify>("flex-start");
  const align = signal<Align>("stretch");
  const colors = ["#fd8c73", "#54aeff", "#4ac26b"];

  return (
    <scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }}>
      <view style={{ padding: 20, paddingTop: 24, gap: 14 }}>
        <view
          onPress={() => direction.set(direction() === "row" ? "column" : "row")}
          style={{ alignSelf: "flex-start", paddingHorizontal: 12, paddingVertical: 6, borderRadius: 6, backgroundColor: "#0969da" }}
        >
          <text style={{ fontSize: 13, color: "#ffffff" }}>flexDirection: {direction()} (tap to switch)</text>
        </view>
        <Choice
          label="justifyContent"
          options={["flex-start", "center", "flex-end", "space-between", "space-evenly"] as const}
          value={justify()}
          onChange={(v) => justify.set(v)}
        />
        <Choice label="alignItems" options={["stretch", "flex-start", "center", "flex-end"] as const} value={align()} onChange={(v) => align.set(v)} />

        <view
          style={{
            height: 220,
            padding: 10,
            gap: 8,
            borderRadius: 12,
            backgroundColor: "#ffffff",
            flexDirection: direction(),
            justifyContent: justify(),
            alignItems: align(),
          }}
        >
          <each of={colors}>
            {(c, i) => (
              <view style={{ minWidth: 44, minHeight: 44, padding: 6, borderRadius: 8, backgroundColor: c() }}>
                <text style={{ color: "#ffffff", fontWeight: "700" }}>{i() + 1}</text>
              </view>
            )}
          </each>
        </view>

        <text style={{ fontSize: 12, fontWeight: "600", color: "#6e7781" }}>flex: 1 / 2 / fixed, percentages, aspectRatio</text>
        <view style={{ flexDirection: "row", gap: 8 }}>
          <view style={{ flex: 1, height: 36, borderRadius: 6, backgroundColor: "#c8e1ff" }} />
          <view style={{ flex: 2, height: 36, borderRadius: 6, backgroundColor: "#79b8ff" }} />
          <view style={{ width: 36, height: 36, borderRadius: 6, backgroundColor: "#0366d6" }} />
        </view>
        <view style={{ flexDirection: "row", gap: 8, alignItems: "flex-start" }}>
          <view style={{ width: "30%", aspectRatio: 1, borderRadius: 8, backgroundColor: "#ffdfb6" }} />
          <view style={{ width: "40%", aspectRatio: 16 / 9, borderRadius: 8, backgroundColor: "#ffab70" }} />
        </view>

        <text style={{ fontSize: 12, fontWeight: "600", color: "#6e7781" }}>grid</text>
        <view style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 6 }}>
          <view style={{ gridColumn: "span 2", height: 40, borderRadius: 6, backgroundColor: "#d8b9ff" }} />
          <view style={{ height: 40, borderRadius: 6, backgroundColor: "#b083f0" }} />
          <view style={{ gridRow: "span 2", borderRadius: 6, backgroundColor: "#8250df" }} />
          <view style={{ gridColumn: "1 / 4", height: 40, borderRadius: 6, backgroundColor: "#6639ba" }} />
        </view>

        <text style={{ fontSize: 12, fontWeight: "600", color: "#6e7781" }}>position: absolute</text>
        <view style={{ height: 100, borderRadius: 12, backgroundColor: "#ffffff" }}>
          <view style={{ position: "absolute", top: 8, left: 8, width: 60, height: 40, borderRadius: 6, backgroundColor: "#4ac26b" }} />
          <view style={{ position: "absolute", right: 8, bottom: 8, width: "40%", height: 30, borderRadius: 6, backgroundColor: "#fd8c73" }} />
          <view style={{ position: "absolute", top: 0, bottom: 0, left: "50%", width: 2, backgroundColor: "#d0d7de" }} />
        </view>
      </view>
    </scroll>
  );
}

mount(App);
