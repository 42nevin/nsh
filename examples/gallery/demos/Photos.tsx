import { createSignal } from "solid-js";
import { createNativeValue } from "nsh/animated";
import { List } from "nsh/list";
import logo from "../logo@2x.png";

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

export function Photos() {
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
