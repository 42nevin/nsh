import { mount, signal, type YNode } from "nsh";
import { createNativeValue } from "nsh/animated";

const palette = ["#0969da", "#8250df", "#bf3989", "#cf222e", "#bc4c00", "#1a7f37"];
const muted = "#57606a";

/** Pages as wide as the scroll view, with a page indicator. */
function Pager() {
  const width = signal(340);
  const page = signal(0);
  return (
    <view style={{ gap: 8 }}>
      <scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 110, borderRadius: 12 }}
        onLayout={(e) => width.set(e.width)}
        onMomentumScrollEnd={(e) => page.set(Math.round(e.x / width()))}
        onScrollEnd={(e) => page.set(Math.round(e.x / width()))}
      >
        <view style={{ flexDirection: "row" }}>
          <each of={palette.slice(0, 4)}>
            {(color, i) => (
              <view style={{ width: width(), height: 110, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <text style={{ color: "#ffffff", fontSize: 24, fontWeight: "700" }}>Page {i() + 1}</text>
              </view>
            )}
          </each>
        </view>
      </scroll>
      <view style={{ flexDirection: "row", justifyContent: "center", gap: 6 }}>
        <each of={[0, 1, 2, 3]}>
          {(i) => <view style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: page() === i() ? "#0969da" : "#d0d7de" }} />}
        </each>
      </view>
    </view>
  );
}

/** Cards that snap: one stop every card plus gap. */
function Cards() {
  return (
    <scroll horizontal showsIndicator={false} snapToInterval={152} decelerationRate="fast" style={{ height: 90 }}>
      <view style={{ flexDirection: "row", gap: 12, paddingHorizontal: 4 }}>
        <each of={Array.from({ length: 12 }, (_, i) => i)}>
          {(i) => (
            <view style={{ width: 140, height: 90, borderRadius: 14, backgroundColor: palette[i() % palette.length], alignItems: "center", justifyContent: "center" }}>
              <text style={{ color: "#ffffff", fontWeight: "700" }}>Card {i() + 1}</text>
            </view>
          )}
        </each>
      </view>
    </scroll>
  );
}

function App() {
  // The vertical offset, written natively in the frame it changes.
  const offset = createNativeValue(0);
  const y = signal(0);
  const refreshing = signal(false);
  const refreshed = signal(0);
  let scroll!: YNode;
  const refresh = () => {
    refreshing.set(true);
    setTimeout(() => {
      refreshed.update((n) => n + 1);
      refreshing.set(false);
    }, 1200);
  };
  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", paddingTop: 24 }}>
      {/* A bar that fades in and shrinks its title as the page scrolls: no JS per frame. */}
      <view
        style={{
          height: 48,
          paddingHorizontal: 16,
          justifyContent: "center",
          backgroundColor: offset.interpolate({ inputRange: [0, 80], outputRange: ["#f6f8fa", "#ffffff"] }),
          borderBottomWidth: 1,
          borderBottomColor: "#d0d7de",
        }}
      >
        <text
          style={{
            fontSize: 22,
            fontWeight: "700",
            transformOrigin: "left center",
            transform: [{ scale: offset.interpolate({ inputRange: [0, 80], outputRange: [1, 0.8] }) }],
          }}
        >
          Scrolling
        </text>
      </view>
      <scroll
        ref={scroll}
        style={{ flex: 1 }}
        scrollValue={offset}
        scrollEventThrottle={100}
        onScroll={(e) => y.set(Math.round(e.y))}
        refreshing={refreshing()}
        onRefresh={refresh}
        keyboardDismissMode="on-drag"
      >
        <view style={{ padding: 16, gap: 18 }}>
          <text style={{ color: muted }}>
            Pull down to refresh ({refreshed()}×). onScroll: y = {y()}
          </text>
          <text style={{ fontWeight: "600" }}>pagingEnabled</text>
          <Pager />
          <text style={{ fontWeight: "600" }}>snapToInterval</text>
          <Cards />
          <each of={Array.from({ length: 24 }, (_, i) => i + 1)}>
            {(n) => (
              <view style={{ padding: 14, borderRadius: 10, backgroundColor: "#ffffff" }}>
                <text>Row {n()}</text>
              </view>
            )}
          </each>
          <view
            onPress={() => scroll.scrollTo({ y: 0 })}
            activeOpacity={0.6}
            style={{ padding: 14, borderRadius: 10, backgroundColor: "#0969da", alignItems: "center" }}
          >
            <text style={{ color: "#ffffff", fontWeight: "600" }}>Back to the top (ref.scrollTo)</text>
          </view>
        </view>
      </scroll>
    </view>
  );
}

mount(App);
