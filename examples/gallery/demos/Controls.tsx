import { createSignal, For } from "solid-js";
import { ActivityIndicator, LayoutAnimation, Modal, Pressable, SafeAreaView, Slider, Switch } from "yf";
import { Button, colors, Section } from "../ui";

export function Controls() {
  const [wifi, setWifi] = createSignal(true);
  const [locked] = createSignal(false);
  const [volume, setVolume] = createSignal(0.4);
  const [loading, setLoading] = createSignal(true);
  const [items, setItems] = createSignal([1, 2, 3]);
  const [sheet, setSheet] = createSignal(false);
  let next = 4;
  const animate = (change: () => void) => {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
    change();
  };
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Switches">
          <view style={{ flexDirection: "row", alignItems: "center" }}>
            <text style={{ flex: 1, fontSize: 17 }}>Wi-Fi {wifi() ? "on" : "off"}</text>
            <Switch value={wifi()} onValueChange={setWifi} trackColor={{ true: "#1a7f37" }} />
          </view>
          <view style={{ flexDirection: "row", alignItems: "center" }}>
            <text style={{ flex: 1, fontSize: 17 }}>Controlled (stays off)</text>
            <Switch value={locked()} onValueChange={() => {}} />
          </view>
        </Section>
        <Section title="Slider">
          <text style={{ fontSize: 17 }}>Volume {Math.round(volume() * 100)}%</text>
          <Slider value={volume()} onValueChange={setVolume} step={0.05} minimumTrackTintColor={colors.accent} />
        </Section>
        <Section title="Activity">
          <view style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
            <ActivityIndicator animating={loading()} />
            <ActivityIndicator animating={loading()} size="large" color="#8250df" />
            <Button label={loading() ? "Stop" : "Start"} onPress={() => setLoading(!loading())} />
          </view>
        </Section>
        <Section title="Layout animation">
          <view style={{ flexDirection: "row", gap: 8 }}>
            <Button label="Add" onPress={() => animate(() => setItems([next++, ...items()]))} />
            <Button label="Shuffle" color="#8250df" onPress={() => animate(() => setItems([...items()].sort(() => Math.random() - 0.5)))} />
            <Button label="Remove" color="#cf222e" onPress={() => animate(() => setItems(items().slice(1)))} />
          </view>
          <view style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
            <For each={items()}>
              {(n) => (
                <view style={{ width: 56, height: 56, borderRadius: 12, backgroundColor: "#ddf4ff", alignItems: "center", justifyContent: "center" }}>
                  <text style={{ fontWeight: "600" }}>{n}</text>
                </view>
              )}
            </For>
          </view>
        </Section>
        <Section title="Pressable and modal">
          <Pressable
            onPress={() => setSheet(true)}
            style={({ pressed }) => ({
              padding: 14,
              borderRadius: 10,
              backgroundColor: pressed ? "#bf3989" : "#8250df",
              transform: [{ scale: pressed ? 0.97 : 1 }],
            })}
          >
            {({ pressed }) => <text style={{ color: "#ffffff", fontWeight: "600" }}>{pressed ? "Release to open" : "Open a sheet"}</text>}
          </Pressable>
        </Section>
      </view>
      <Modal visible={sheet()} presentationStyle="pageSheet" animationType="slide" onRequestClose={() => setSheet(false)}>
        <SafeAreaView style={{ flex: 1, backgroundColor: colors.surface, padding: 20, gap: 16 }}>
          <text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</text>
          <text style={{ color: colors.muted }}>Swipe down (iOS) or press back (Android) to close.</text>
          <Button label="Close" onPress={() => setSheet(false)} />
        </SafeAreaView>
      </Modal>
    </scroll>
  );
}
