import { createSignal, Show } from "solid-js";
import { createNativeValue, List, render, root } from "yf";
import logo from "./logo@2x.png";

type Photo = { id: number; title: string; author: string };

function makePhotos(from: number, count: number): Photo[] {
  return Array.from({ length: count }, (_, i) => {
    const id = from + i;
    return { id, title: `Photo #${id}`, author: ["Ada", "Linus", "Grace", "Ken", "Barbara"][id % 5] };
  });
}

function Row(props: { photo: Photo }) {
  return (
    <view style={{ flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 16, paddingVertical: 8 }}>
      <image
        src={`https://picsum.photos/seed/${props.photo.id}/96/96`}
        style={{ width: 48, height: 48, borderRadius: 8, backgroundColor: "#e1e4e8" }}
      />
      <view style={{ flex: 1, gap: 2 }}>
        <text style={{ fontSize: 16, fontWeight: "600", color: "#1f2328" }}>{props.photo.title}</text>
        <text style={{ fontSize: 13, color: "#6e7781" }}>by {props.photo.author}</text>
      </view>
    </view>
  );
}

function Photos() {
  const [photos, setPhotos] = createSignal(makePhotos(1, 1000));
  const [refreshing, setRefreshing] = createSignal(false);
  // Scroll offset, fed natively: the header fades without any JS per frame.
  const scrollY = createNativeValue(0);

  const refresh = () => {
    setRefreshing(true);
    setTimeout(() => {
      setPhotos(makePhotos(1, 1000));
      setRefreshing(false);
    }, 800);
  };
  const loadMore = () => {
    const current = photos();
    console.log("loading more after", current.length);
    setPhotos([...current, ...makePhotos(current.length + 1, 200)]);
  };

  return (
    <view style={{ flex: 1 }}>
      <view
        style={{
          paddingHorizontal: 16,
          paddingVertical: 10,
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          opacity: scrollY.interpolate({ inputRange: [0, 120], outputRange: [1, 0.35] }),
          transform: [{ translateY: scrollY.interpolate({ inputRange: [0, 120], outputRange: [0, -6] }) }],
        }}
      >
        <image src={logo} />
        <text style={{ fontSize: 22, fontWeight: "bold", color: "#1f2328" }}>{photos().length} photos</text>
      </view>
      <List
        each={photos()}
        estimatedItemSize={64}
        style={{ flex: 1, backgroundColor: "#ffffff" }}
        scrollValue={scrollY}
        refreshing={refreshing()}
        onRefresh={refresh}
        onEndReached={loadMore}
      >
        {(photo) => <Row photo={photo()} />}
      </List>
    </view>
  );
}

function Physics() {
  const x = createNativeValue(0);
  const y = createNativeValue(0);
  const pressed = createNativeValue(0);
  const [releases, setReleases] = createSignal(0);

  const release = (e: { velocityX: number; velocityY: number }) => {
    setReleases(releases() + 1);
    x.spring(0, { velocity: e.velocityX, stiffness: 180, damping: 14 });
    y.spring(0, { velocity: e.velocityY, stiffness: 180, damping: 14 });
  };
  const pulse = async () => {
    await pressed.timing(1, { duration: 120, easing: "easeOut" });
    await pressed.timing(0, { duration: 260, easing: "easeInOut" });
  };

  return (
    <view style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 24 }}>
      <text style={{ fontSize: 15, color: "#6e7781" }}>Drag the card, tap it to pulse, long-press to reset</text>
      <view
        pan={{ x, y }}
        onPanEnd={release}
        onPress={pulse}
        onLongPress={() => setReleases(0)}
        style={{
          width: 160,
          height: 160,
          borderRadius: 28,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: pressed.interpolate({ inputRange: [0, 1], outputRange: ["#8250df", "#bf3989"] }),
          transform: [
            { translateX: x },
            { translateY: y },
            { scale: pressed.interpolate({ inputRange: [0, 1], outputRange: [1, 1.12] }) },
            { rotate: x.interpolate({ inputRange: [-200, 200], outputRange: [-12, 12] }) },
          ],
        }}
      >
        <text style={{ color: "#ffffff", fontSize: 20, fontWeight: "bold" }}>Drag me</text>
      </view>
      <text style={{ fontSize: 13, color: "#6e7781" }}>Released {releases()} times</text>
    </view>
  );
}

function App() {
  const [tab, setTab] = createSignal<"photos" | "physics">("photos");
  const Tab = (props: { id: "photos" | "physics"; label: string }) => (
    <view
      onPress={() => setTab(props.id)}
      style={{
        flex: 1,
        paddingVertical: 8,
        alignItems: "center",
        borderRadius: 8,
        backgroundColor: tab() === props.id ? "#ffffff" : "transparent",
      }}
    >
      <text style={{ fontSize: 14, fontWeight: "600", color: tab() === props.id ? "#1f2328" : "#6e7781" }}>
        {props.label}
      </text>
    </view>
  );
  return (
    <view style={{ flex: 1, backgroundColor: "#f4f5f7", paddingTop: 52 }}>
      <view style={{ flexDirection: "row", marginHorizontal: 16, marginBottom: 8, padding: 3, borderRadius: 10, backgroundColor: "#e1e4e8" }}>
        <Tab id="photos" label="Photos" />
        <Tab id="physics" label="Physics" />
      </view>
      <Show when={tab() === "photos"} fallback={<Physics />}>
        <Photos />
      </Show>
    </view>
  );
}

render(() => <App />, root);
