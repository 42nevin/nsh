import { createSignal, For } from "solid-js";

const card = {
  width: 150,
  height: 110,
  alignItems: "center",
  justifyContent: "center",
  backgroundColor: "#ffffff",
} as const;

function Label(props: { children: string }) {
  return <text style={{ fontSize: 13, fontWeight: "600", color: "#1f2328" }}>{props.children}</text>;
}

export function Styles() {
  const [clip, setClip] = createSignal(true);
  const [front, setFront] = createSignal(0);
  return (
    <scroll style={{ flex: 1 }}>
    <view style={{ flexDirection: "row", flexWrap: "wrap", gap: 24, padding: 24, justifyContent: "center" }}>
      <view style={{ ...card, borderRadius: 16, shadowOpacity: 0.18, shadowRadius: 12, shadowOffset: { width: 0, height: 6 } }}>
        <Label>Shadow</Label>
      </view>
      <view
        style={{
          ...card,
          borderTopLeftRadius: 28,
          borderTopRightRadius: 28,
          borderWidth: 2,
          borderColor: "#8250df",
        }}
      >
        <Label>Top corners</Label>
      </view>
      <view
        style={{
          ...card,
          backgroundColor: "#ddf4ff",
          borderTopLeftRadius: 44,
          borderTopRightRadius: 6,
          borderBottomRightRadius: 28,
          borderWidth: 3,
          borderColor: "#0969da",
          shadowColor: "#0969da",
          shadowOpacity: 0.35,
          shadowRadius: 10,
          shadowOffset: { width: 4, height: 8 },
        }}
      >
        <Label>Mixed radii</Label>
      </view>
      <view
        onPress={() => setClip(!clip())}
        style={{
          ...card,
          backgroundColor: "#fff8c5",
          borderRadius: 24,
          borderWidth: 2,
          borderColor: "#9a6700",
          overflow: clip() ? "hidden" : "visible",
          shadowOpacity: 0.3,
          shadowRadius: 8,
          shadowOffset: { width: 0, height: 4 },
        }}
      >
        <view style={{ position: "absolute", left: -30, top: -30, width: 90, height: 90, borderRadius: 45, backgroundColor: "#bf3989" }} />
        <Label>{clip() ? "Clipped (tap)" : "Visible (tap)"}</Label>
      </view>
      <image
        src="https://picsum.photos/seed/yf/300/220"
        style={{
          width: 150,
          height: 110,
          borderTopLeftRadius: 36,
          borderBottomRightRadius: 36,
          backgroundColor: "#e1e4e8",
          shadowOpacity: 0.3,
          shadowRadius: 10,
          shadowOffset: { width: 0, height: 6 },
        }}
      />
      <view style={{ ...card, backgroundColor: "transparent", shadowOpacity: 0.5, shadowRadius: 3, shadowOffset: { width: 2, height: 2 } }}>
        <text style={{ fontSize: 22, fontWeight: "bold", color: "#1f2328" }}>Text shadow</text>
      </view>
      <view style={{ width: 150, alignItems: "center" }}>
        <text
          style={{
            fontSize: 15,
            color: "#ffffff",
            backgroundColor: "#1a7f37",
            borderTopLeftRadius: 14,
            borderBottomRightRadius: 14,
            paddingHorizontal: 12,
            paddingVertical: 6,
          }}
        >
          Text corners
        </text>
      </view>
      <view
        style={{
          ...card,
          borderTopWidth: 6,
          borderTopColor: "#cf222e",
          borderRightWidth: 2,
          borderRightColor: "#1a7f37",
          borderBottomWidth: 4,
          borderBottomColor: "#0969da",
          borderLeftWidth: 1,
          borderLeftColor: "#8250df",
        }}
      >
        <Label>Per-side borders</Label>
      </view>
      <view style={{ ...card, gap: 8 }}>
        <view style={{ width: 120, height: 30, borderRadius: 8, borderWidth: 2, borderColor: "#0969da", borderStyle: "dashed" }} />
        <view style={{ width: 120, height: 30, borderRadius: 8, borderWidth: 3, borderColor: "#bf3989", borderStyle: "dotted" }} />
        <Label>Dashed / dotted</Label>
      </view>
      <view onPress={() => setFront((front() + 1) % 3)} style={{ ...card, backgroundColor: "transparent" }}>
        <For each={["#fd8c73", "#54aeff", "#4ac26b"]}>
          {(color, i) => (
            <view
              style={{
                position: "absolute",
                left: 20 + i() * 30,
                top: 10 + i() * 22,
                width: 70,
                height: 56,
                borderRadius: 10,
                backgroundColor: color,
                zIndex: front() === i() ? 10 : 0,
                shadowOpacity: 0.2,
                shadowRadius: 4,
                shadowOffset: { width: 0, height: 2 },
              }}
            />
          )}
        </For>
        <view style={{ position: "absolute", bottom: -18 }}>
          <Label>zIndex (tap)</Label>
        </view>
      </view>
      <view style={{ ...card, backgroundColor: "#ddf4ff", transform: [{ perspective: 400 }, { rotateY: "35deg" }] }}>
        <Label>rotateY + perspective</Label>
      </view>
      <view style={{ ...card, backgroundColor: "#fbefff", transform: [{ perspective: 400 }, { rotateX: "40deg" }] }}>
        <Label>rotateX</Label>
      </view>
      <view style={{ ...card, backgroundColor: "#fff1e5", transform: [{ skewX: "-12deg" }] }}>
        <Label>skewX (iOS)</Label>
      </view>
      <view style={{ ...card, backgroundColor: "#1f2328", opacity: 0.45 }}>
        <text style={{ color: "#ffffff", fontWeight: "600" }}>opacity 0.45</text>
      </view>
    </view>
    </scroll>
  );
}
