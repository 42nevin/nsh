import { createSignal, For } from "solid-js";
import logo from "../logo@2x.png";
import { colors, Section } from "../ui";

const wide = "https://picsum.photos/seed/yf-wide/480/240";

function Caption(props: { children: string }) {
  return <text style={{ fontSize: 12, color: colors.muted, textAlign: "center" }}>{props.children}</text>;
}

export function Images() {
  const [loaded, setLoaded] = createSignal("loading…");
  const [failed, setFailed] = createSignal("loading…");
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Resize modes (a 2:1 image in squares)">
          <view style={{ flexDirection: "row", flexWrap: "wrap", gap: 12, justifyContent: "space-between" }}>
            <For each={["cover", "contain", "stretch", "center"] as const}>
              {(mode) => (
                <view style={{ gap: 4 }}>
                  <image src={wide} resizeMode={mode} style={{ width: 140, height: 140, backgroundColor: "#eaeef2" }} />
                  <Caption>{mode}</Caption>
                </view>
              )}
            </For>
          </view>
        </Section>
        <Section title="Corners and borders">
          <view style={{ flexDirection: "row", gap: 12, justifyContent: "center" }}>
            <image src={wide} style={{ width: 90, height: 90, borderRadius: 45 }} />
            <image src={wide} style={{ width: 90, height: 90, borderRadius: 16, borderWidth: 3, borderColor: "#8250df" }} />
            <image src={wide} style={{ width: 90, height: 90, borderTopLeftRadius: 40, borderBottomRightRadius: 40 }} />
          </view>
        </Section>
        <Section title="Assets and tint">
          <view style={{ flexDirection: "row", gap: 16, alignItems: "center", justifyContent: "center" }}>
            <view style={{ gap: 4, alignItems: "center" }}>
              <image src={logo} />
              <Caption>logo@2x.png</Caption>
            </view>
            <For each={["#cf222e", "#1a7f37", "#0969da"]}>
              {(tint) => (
                <view style={{ gap: 4, alignItems: "center" }}>
                  <image src={logo} style={{ tintColor: tint }} />
                  <Caption>{tint}</Caption>
                </view>
              )}
            </For>
          </view>
        </Section>
        <Section title="Events">
          <view style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
            <image
              src="https://picsum.photos/seed/yf-load/300/200"
              onLoad={(e) => setLoaded(`onLoad: ${e.width}×${e.height}`)}
              onError={(e) => setLoaded(`onError: ${e.message}`)}
              style={{ width: 90, height: 60, borderRadius: 8, backgroundColor: "#eaeef2" }}
            />
            <text style={{ flex: 1 }}>{loaded()}</text>
          </view>
          <view style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
            <image
              src="https://example.invalid/missing.png"
              onError={(e) => setFailed(`onError: ${e.message}`)}
              style={{ width: 90, height: 60, borderRadius: 8, backgroundColor: "#eaeef2" }}
            />
            <text numberOfLines={3} style={{ flex: 1 }}>{failed()}</text>
          </view>
        </Section>
      </view>
    </scroll>
  );
}
