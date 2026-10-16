import { mount, signal, type YNode } from "nsh";

const colors = ["#0969da", "#8250df", "#bf3989", "#cf222e", "#bc4c00", "#1a7f37"];

function App() {
  const page = signal(0);
  const y = signal(0);
  let scroller!: YNode;
  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", paddingTop: 24, gap: 12 }}>
      <text style={{ fontSize: 22, fontWeight: "700", color: "#1f2328", paddingHorizontal: 20 }}>scroll</text>

      <text style={{ color: "#57606a", paddingHorizontal: 20 }}>Horizontal, pagingEnabled · page {page() + 1}</text>
      <scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 140, flexGrow: 0 }}
        onMomentumScrollEnd={(e) => page.set(Math.round(e.x / 390))}
      >
        <each of={colors}>
          {(color, index) => (
            <view style={{ width: 390, padding: 12 }}>
              <view style={{ flex: 1, borderRadius: 16, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <text style={{ color: "#ffffff", fontSize: 32, fontWeight: "700" }}>{index() + 1}</text>
              </view>
            </view>
          )}
        </each>
      </scroll>

      <view style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingHorizontal: 20 }}>
        <text style={{ color: "#57606a" }}>Vertical · y = {Math.round(y())}</text>
        <view
          onPress={() => scroller.scrollTo({ y: 0, animated: true })}
          activeOpacity={0.6}
          style={{ backgroundColor: "#ddf4ff", borderRadius: 8, paddingHorizontal: 12, paddingVertical: 6 }}
        >
          <text style={{ color: "#0969da" }}>scrollTo top</text>
        </view>
      </view>
      <scroll ref={scroller} style={{ flex: 1 }} scrollEventThrottle={16} onScroll={(e) => y.set(e.y)}>
        <view style={{ paddingHorizontal: 20, paddingBottom: 20, gap: 8 }}>
          <each of={Array.from({ length: 40 }, (_, i) => i + 1)}>
            {(n) => (
              <view style={{ backgroundColor: "#ffffff", borderRadius: 10, padding: 14, borderWidth: 1, borderColor: "#d0d7de" }}>
                <text style={{ color: "#1f2328" }}>Row {n()}</text>
              </view>
            )}
          </each>
        </view>
      </scroll>
    </view>
  );
}

mount(App);
