import { createSignal, For, Index } from "solid-js";
import { Image } from "nsh";
import bounce from "../bounce.gif";
import logo from "../logo@2x.png";
import spinner from "../spinner.webp";
import { Button, colors, Section } from "../ui";

const wide = "https://picsum.photos/seed/nsh-wide/480/240";

function Caption(props: { children: string }) {
  return <text style={{ fontSize: 12, color: colors.muted, textAlign: "center" }}>{props.children}</text>;
}

/** Network images behind a placeholder, fading in; "Reload" asks for
 * images that are not cached yet. */
function Placeholders() {
  const [seed, setSeed] = createSignal(1);
  const fades = [0, 300, 1000];
  return (
    <view style={{ gap: 10 }}>
      <view style={{ flexDirection: "row", justifyContent: "space-between" }}>
        <Index each={fades}>
          {(fade) => (
            <view style={{ gap: 4, alignItems: "center" }}>
              <image
                src={`https://picsum.photos/seed/nsh-fade-${seed()}-${fade()}/300/300`}
                defaultSource={logo}
                fadeDuration={fade()}
                resizeMode="cover"
                style={{ width: 100, height: 100, borderRadius: 12, backgroundColor: "#eaeef2" }}
              />
              <Caption>{`fade ${fade()}ms`}</Caption>
            </view>
          )}
        </Index>
      </view>
      <Button label="Reload" onPress={() => setSeed(seed() + 1)} />
    </view>
  );
}

function Blur() {
  const radii = [0, 2, 6, 16];
  return (
    <view style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <Index each={radii}>
        {(radius) => (
          <view style={{ gap: 4, alignItems: "center" }}>
            <image src={wide} blurRadius={radius()} style={{ width: 76, height: 76, borderRadius: 8 }} />
            <Caption>{`blur ${radius()}`}</Caption>
          </view>
        )}
      </Index>
    </view>
  );
}

function Info() {
  const big = "https://picsum.photos/seed/nsh-progress/2400/1600";
  const [size, setSize] = createSignal("");
  const [prefetched, setPrefetched] = createSignal("");
  const [progress, setProgress] = createSignal("");
  const [show, setShow] = createSignal(false);
  return (
    <view style={{ gap: 10 }}>
      <view style={{ flexDirection: "row", gap: 8 }}>
        <Button
          label="getSize"
          onPress={() =>
            Image.getSize(big).then(
              (s) => setSize(`${s.width}×${s.height} px`),
              (e) => setSize(String(e)),
            )
          }
        />
        <Button label="prefetch" color="#8250df" onPress={() => Image.prefetch(big).then((ok) => setPrefetched(ok ? "prefetched" : "failed"))} />
        <Button label={show() ? "Hide" : "Show"} color="#1a7f37" onPress={() => setShow(!show())} />
      </view>
      <text style={{ color: colors.muted }}>
        {[size(), prefetched(), progress()].filter(Boolean).join(" · ") || "Prefetch, then show: no download, no progress"}
      </text>
      {show() && (
        <image
          src={big}
          onProgress={(e) => setProgress(`${Math.round((e.loaded / Math.max(1, e.total)) * 100)}%`)}
          onLoad={() => setProgress((p) => (p ? `${p} loaded` : "loaded (no download)"))}
          style={{ height: 160, borderRadius: 10, backgroundColor: "#eaeef2" }}
        />
      )}
    </view>
  );
}

export function Images() {
  const [events, setEvents] = createSignal<string[]>([]);
  const log = (line: string) => setEvents([...events(), line]);
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
        <Section title="Animated GIF and WebP">
          <view style={{ flexDirection: "row", gap: 24, justifyContent: "center" }}>
            <view style={{ gap: 4, alignItems: "center" }}>
              <image src={bounce} />
              <Caption>bounce.gif</Caption>
            </view>
            <view style={{ gap: 4, alignItems: "center" }}>
              <image src={spinner} />
              <Caption>spinner.webp</Caption>
            </view>
            <view style={{ gap: 4, alignItems: "center" }}>
              <image src={bounce} style={{ width: 48, height: 48, borderRadius: 24 }} />
              <Caption>scaled, round</Caption>
            </view>
          </view>
        </Section>
        <Section title="Placeholder (defaultSource) and fade in">
          <Placeholders />
        </Section>
        <Section title="Blur radius">
          <Blur />
        </Section>
        <Section title="Image.getSize, Image.prefetch, onProgress">
          <Info />
        </Section>
        <Section title="Events">
          <view style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
            <image
              src="https://picsum.photos/seed/nsh-load/300/200"
              onLoadStart={() => log("onLoadStart")}
              onLoad={(e) => {
                log("onLoad");
                setLoaded(`onLoad: ${e.width}×${e.height}`);
              }}
              onError={(e) => setLoaded(`onError: ${e.message}`)}
              onLoadEnd={() => log("onLoadEnd")}
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
          <Caption>{events().join(" → ") || "no load events yet"}</Caption>
        </Section>
      </view>
    </scroll>
  );
}
