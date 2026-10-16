import { mount, signal } from "nsh";

function App() {
  const presses = signal(0);
  const size = signal("");
  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", padding: 20, paddingTop: 24, gap: 16 }}>
      <text style={{ fontSize: 22, fontWeight: "700", color: "#1f2328" }}>view</text>

      <view
        onPress={() => presses.update((n) => n + 1)}
        activeOpacity={0.5}
        style={{ backgroundColor: "#0969da", borderRadius: 12, padding: 16, alignItems: "center" }}
      >
        <text style={{ color: "#ffffff", fontSize: 16, fontWeight: "600" }}>activeOpacity · pressed {presses()}×</text>
      </view>

      <view
        onPress={() => presses.update((n) => n + 1)}
        underlayColor="#d0d7de"
        style={{ backgroundColor: "#ffffff", borderRadius: 12, padding: 16, borderWidth: 1, borderColor: "#d0d7de" }}
      >
        <text style={{ color: "#1f2328", fontSize: 16 }}>underlayColor (tap and hold)</text>
      </view>

      <view class="flex-row gap-3">
        <view class="flex-1 h-20 rounded-xl bg-rose-500 active:bg-rose-700 items-center justify-center" onPress={() => {}}>
          <text class="text-white font-semibold">active:</text>
        </view>
        <view class="flex-1 h-20 rounded-xl bg-emerald-500 items-center justify-center shadow-lg">
          <text class="text-white font-semibold">shadow-lg</text>
        </view>
        <view
          class="flex-1 h-20 rounded-xl items-center justify-center"
          style={{ backgroundImage: "linear-gradient(135deg, #8250df, #0969da)" }}
        >
          <text class="text-white font-semibold">gradient</text>
        </view>
      </view>

      <view
        onLayout={(e) => size.set(`${Math.round(e.width)} × ${Math.round(e.height)} pt`)}
        style={{ flex: 1, borderRadius: 12, borderWidth: 2, borderStyle: "dashed", borderColor: "#8c959f", alignItems: "center", justifyContent: "center" }}
      >
        <text style={{ color: "#57606a" }}>onLayout: {size()}</text>
      </view>
    </view>
  );
}

mount(App);
