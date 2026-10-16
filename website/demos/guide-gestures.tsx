import { mount, signal } from "nsh";
import { createNativeValue } from "nsh/animated";
import { PanResponder } from "nsh/gesture";

const muted = "#57606a";

/** A card dragged on the UI thread: `pan` drives two native values. */
function DragCard() {
  const x = createNativeValue(0);
  const y = createNativeValue(0);
  const flings = signal(0);
  return (
    <view style={{ height: 150, borderRadius: 12, backgroundColor: "#eaeef2", alignItems: "center", justifyContent: "center", overflow: "hidden" }}>
      <view
        pan={{ x, y }}
        onPanEnd={(e) => {
          flings.update((n) => n + 1);
          // Spring home, starting with the release velocity.
          x.spring(0, { velocity: e.velocityX, stiffness: 200, damping: 16 });
          y.spring(0, { velocity: e.velocityY, stiffness: 200, damping: 16 });
        }}
        style={{
          width: 120,
          height: 80,
          borderRadius: 16,
          backgroundColor: "#8250df",
          alignItems: "center",
          justifyContent: "center",
          transform: [{ translateX: x }, { translateY: y }],
        }}
      >
        <text style={{ color: "#ffffff", fontWeight: "700" }}>Drag me</text>
        <text style={{ color: "#ffffff", fontSize: 12 }}>released {flings()}×</text>
      </view>
    </view>
  );
}

/** A dot moved with PanResponder: every move goes through JS. */
function ResponderDot() {
  const pos = signal({ x: 0, y: 0 });
  const state = signal("idle");
  let start = { x: 0, y: 0 };
  const responder = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onPanResponderGrant: () => {
      start = pos();
      state.set("dragging");
    },
    onPanResponderMove: (_, g) => pos.set({ x: start.x + g.dx, y: start.y + g.dy }),
    onPanResponderRelease: (_, g) => state.set(`released, vx ${g.vx.toFixed(2)} pt/ms`),
    onPanResponderTerminate: () => state.set("taken over"),
  });
  return (
    <view style={{ gap: 6 }}>
      <view style={{ height: 110, borderRadius: 12, backgroundColor: "#eaeef2", overflow: "hidden" }}>
        <view
          {...responder.panHandlers}
          style={{
            position: "absolute",
            left: 150,
            top: 30,
            width: 50,
            height: 50,
            borderRadius: 25,
            backgroundColor: "#0969da",
            transform: [{ translateX: pos().x }, { translateY: pos().y }],
          }}
        />
      </view>
      <text style={{ color: muted, fontSize: 13 }}>PanResponder: {state()}</text>
    </view>
  );
}

function App() {
  const log = signal<string[]>([]);
  const add = (event: string) => log.update((l) => [event, ...l].slice(0, 4));
  const dot = signal(0);
  const under = signal(0);
  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", padding: 16, paddingTop: 24, gap: 14 }}>
      <view style={{ flexDirection: "row", gap: 10 }}>
        <view
          onPressIn={() => add("pressIn")}
          onPressOut={() => add("pressOut")}
          onPress={() => add("press")}
          onLongPress={() => add("longPress")}
          activeOpacity={0.5}
          style={{ flex: 1, padding: 14, borderRadius: 12, backgroundColor: "#0969da", alignItems: "center" }}
        >
          <text style={{ color: "#ffffff", fontWeight: "600" }}>activeOpacity</text>
        </view>
        <view
          onPress={() => add("underlay")}
          underlayColor="#ddf4ff"
          style={{ flex: 1, padding: 14, borderRadius: 12, backgroundColor: "#ffffff", borderWidth: 1, borderColor: "#d0d7de", alignItems: "center" }}
        >
          <text style={{ fontWeight: "600" }}>underlayColor</text>
        </view>
        <view
          onPress={() => add("active:")}
          class="flex-1 items-center rounded-xl bg-emerald-600 p-3.5 transition active:scale-95 active:bg-emerald-800"
        >
          <text class="font-semibold text-white">active:</text>
        </view>
      </view>
      <text style={{ color: muted, fontSize: 13 }}>{log().join(" · ") || "Tap, hold, or slide off a button"}</text>

      <view style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
        <view
          hitSlop={24}
          onPress={() => dot.update((n) => n + 1)}
          style={{ width: 12, height: 12, borderRadius: 6, backgroundColor: "#cf222e", margin: 12 }}
        />
        <text style={{ flex: 1 }}>hitSlop 24: the dot takes taps around it ({dot()})</text>
      </view>

      <view style={{ height: 52 }}>
        <view
          onPress={() => under.update((n) => n + 1)}
          style={{ flex: 1, borderRadius: 12, backgroundColor: "#1a7f37", alignItems: "center", justifyContent: "center" }}
        >
          <text style={{ color: "#ffffff" }}>Under an overlay: {under()}</text>
        </view>
        <view
          pointerEvents="none"
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, borderRadius: 12, borderWidth: 3, borderColor: "#bf8700", borderStyle: "dashed" }}
        />
      </view>

      <DragCard />
      <ResponderDot />
    </view>
  );
}

mount(App);
