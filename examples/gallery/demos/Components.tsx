import { createSignal, For, Show } from "solid-js";
import {
  AccessibilityInfo,
  DrawerLayout,
  type DrawerLayoutHandle,
  type DrawerState,
  ImageBackground,
  InputAccessoryView,
  KeyboardAvoidingView,
  LayoutAnimation,
  Modal,
  PixelRatio,
  Platform,
  Pressable,
  SafeAreaView,
  Screen,
  screenReaderEnabled,
  reduceMotion,
  StyleSheet,
  TouchableHighlight,
  TouchableOpacity,
  TouchableWithoutFeedback,
} from "yf";
import { nav } from "../nav";
import { Button as UiButton, colors, Section } from "../ui";

const photo = "https://picsum.photos/seed/yf-components/800/500";

const styles = StyleSheet.create({
  row: { flexDirection: "row", alignItems: "center", gap: 12 },
  card: { backgroundColor: "#ffffff", borderRadius: 10, padding: 12, borderWidth: StyleSheet.hairlineWidth, borderColor: colors.border },
  label: { fontSize: 15, color: colors.text },
  muted: { fontSize: 13, color: colors.muted },
});

function LayoutAnimations() {
  const [items, setItems] = createSignal([1, 2, 3]);
  let next = 4;
  const animate = (change: () => void) => {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
    change();
  };
  return (
    <Section title="Layout animation">
      <view style={styles.row}>
        <UiButton label="Add" onPress={() => animate(() => setItems([next++, ...items()]))} />
        <UiButton label="Shuffle" color="#8250df" onPress={() => animate(() => setItems([...items()].sort(() => Math.random() - 0.5)))} />
        <UiButton label="Remove" color="#cf222e" onPress={() => animate(() => setItems(items().slice(1)))} />
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
  );
}

function PressableSheet() {
  const [sheet, setSheet] = createSignal(false);
  return (
    <Section title="Pressable and sheet">
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
      <Modal visible={sheet()} presentationStyle="pageSheet" animationType="slide" onRequestClose={() => setSheet(false)}>
        <SafeAreaView style={{ flex: 1, backgroundColor: colors.surface, padding: 20, gap: 16 }}>
          <text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</text>
          <text style={{ color: colors.muted }}>Swipe down (iOS) or press back (Android) to close.</text>
          <UiButton label="Close" onPress={() => setSheet(false)} />
        </SafeAreaView>
      </Modal>
    </Section>
  );
}

function Touchables() {
  const [last, setLast] = createSignal("nothing yet");
  return (
    <Section title="Touchables">
      <TouchableOpacity style={styles.card} onPress={() => setLast("TouchableOpacity")}>
        <text style={styles.label}>TouchableOpacity: fades while pressed</text>
      </TouchableOpacity>
      <TouchableHighlight style={styles.card} underlayColor="#dbeafe" onPress={() => setLast("TouchableHighlight")}>
        <text style={styles.label}>TouchableHighlight: underlay color, no JS round trip</text>
      </TouchableHighlight>
      <TouchableWithoutFeedback onPress={() => setLast("TouchableWithoutFeedback")}>
        <text style={[styles.label, { paddingVertical: 4 }]}>TouchableWithoutFeedback: props go to this text</text>
      </TouchableWithoutFeedback>
      <text style={styles.muted}>Last pressed: {last()}</text>
    </Section>
  );
}

function Background() {
  return (
    <Section title="ImageBackground">
      <ImageBackground src={photo} resizeMode="cover" style={{ height: 140, justifyContent: "flex-end" }} imageStyle={{ borderRadius: 10 }}>
        <view style={{ margin: 10, padding: 8, borderRadius: 8, backgroundColor: "rgba(0,0,0,0.45)" }}>
          <text style={{ color: "#ffffff", fontWeight: "600" }}>Content over an image</text>
        </view>
      </ImageBackground>
    </Section>
  );
}

function Drawer() {
  let drawer: DrawerLayoutHandle | undefined;
  const [side, setSide] = createSignal<"left" | "right">("left");
  const [state, setState] = createSignal<DrawerState>("Idle");
  const [opened, setOpened] = createSignal(0);
  const items = ["Inbox", "Starred", "Sent", "Drafts", "Trash"];
  return (
    <Section title="DrawerLayout">
      <view style={{ height: 260, borderRadius: 10, overflow: "hidden", borderWidth: StyleSheet.hairlineWidth, borderColor: colors.border }}>
        <DrawerLayout
          ref={(d) => (drawer = d)}
          drawerPosition={side()}
          drawerWidth={200}
          drawerBackgroundColor="#f6f8fa"
          onDrawerOpen={() => setOpened(opened() + 1)}
          onDrawerStateChanged={setState}
          renderNavigationView={() => (
            <view style={{ paddingVertical: 12 }}>
              <For each={items}>
                {(item) => (
                  <TouchableHighlight underlayColor="#eaeef2" onPress={() => drawer?.closeDrawer()} style={{ paddingHorizontal: 16, paddingVertical: 10 }}>
                    <text style={styles.label}>{item}</text>
                  </TouchableHighlight>
                )}
              </For>
            </view>
          )}
        >
          <view style={{ flex: 1, padding: 16, gap: 10, backgroundColor: "#ffffff" }}>
            <text style={styles.label}>Drag from the {side()} edge, or:</text>
            <view style={styles.row}>
              <UiButton label="Open" onPress={() => drawer?.openDrawer()} />
              <UiButton label={side() === "left" ? "Right side" : "Left side"} color="#57606a" onPress={() => setSide(side() === "left" ? "right" : "left")} />
            </view>
            <text style={styles.muted}>
              State: {state()} · opened {opened()} times. Drags run natively; JS only picks where it settles.
            </text>
          </view>
        </DrawerLayout>
      </view>
    </Section>
  );
}

function Accessory() {
  const [text, setText] = createSignal("");
  let input: any;
  return (
    <Section title="InputAccessoryView">
      <input
        ref={input}
        value={text()}
        onChangeText={setText}
        placeholder="Focus me (iOS shows a bar over the keyboard)"
        inputAccessoryViewID="counter"
        style={{ borderWidth: 1, borderColor: colors.border, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 8, fontSize: 16 }}
      />
      <Show when={Platform.OS === "android"}>
        <text style={styles.muted}>Android has no input accessories (as React Native).</text>
      </Show>
      <InputAccessoryView nativeID="counter" backgroundColor="#f6f8fa">
        <view style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: 12, paddingVertical: 8, borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: colors.border }}>
          <text style={[styles.muted, { flex: 1 }]}>{text().length} characters</text>
          <UiButton label="Clear" color="#57606a" onPress={() => setText("")} />
          <view style={{ width: 8 }} />
          <UiButton label="Done" onPress={() => input?.blur()} />
        </view>
      </InputAccessoryView>
    </Section>
  );
}

function AvoidingPage() {
  const [behavior, setBehavior] = createSignal<"padding" | "height" | "position">("padding");
  return (
    <Screen title="KeyboardAvoidingView">
      <KeyboardAvoidingView behavior={behavior()} style={{ flex: 1, backgroundColor: "#ddf4ff" }} contentContainerStyle={{ flex: 1 }}>
        <view style={{ flex: 1, padding: 16, gap: 12 }}>
          <text style={styles.label}>behavior="{behavior()}"</text>
          <view style={styles.row}>
            <For each={["padding", "height", "position"] as const}>
              {(b) => <UiButton label={b} color={behavior() === b ? colors.accent : "#57606a"} onPress={() => setBehavior(b)} />}
            </For>
          </view>
          <text style={styles.muted}>
            padding: the bottom pads (the blue background stays under the keyboard); height: the view shrinks; position: the content moves up.
          </text>
          <view style={{ flex: 1 }} />
          <input
            placeholder="Type here"
            style={{ backgroundColor: "#ffffff", borderRadius: 8, paddingHorizontal: 10, paddingVertical: 10, fontSize: 16 }}
          />
        </view>
      </KeyboardAvoidingView>
    </Screen>
  );
}

function ModalOptions() {
  const [visible, setVisible] = createSignal(false);
  const [translucent, setTranslucent] = createSignal(false);
  return (
    <Section title="Modal options">
      <view style={styles.row}>
        <UiButton label="Open" onPress={() => setVisible(true)} />
        <UiButton
          label={`statusBarTranslucent: ${translucent()}`}
          color="#57606a"
          onPress={() => setTranslucent(!translucent())}
        />
      </view>
      <text style={styles.muted}>
        Android: translucent content reaches under the status bar. iOS: this modal allows portrait and landscape.
      </text>
      <Modal
        visible={visible()}
        animationType="slide"
        statusBarTranslucent={translucent()}
        supportedOrientations={["portrait", "landscape"]}
        onRequestClose={() => setVisible(false)}
      >
        <view style={{ flex: 1, backgroundColor: "#fff8c5" }}>
          <SafeAreaView style={{ flex: 1, padding: 16, gap: 12 }}>
            <text style={{ fontSize: 22, fontWeight: "700" }}>A modal</text>
            <text style={styles.muted}>The yellow background starts {translucent() ? "under" : "below"} the status bar on Android.</text>
            <UiButton label="Close" onPress={() => setVisible(false)} />
          </SafeAreaView>
        </view>
      </Modal>
    </Section>
  );
}

function PlatformInfo() {
  const rows = (): [string, string][] => [
    ["Platform.OS", Platform.OS],
    ["Platform.Version", String(Platform.Version)],
    ["isPad / isTV / isMacCatalyst", `${Platform.isPad} / ${Platform.isTV} / ${Platform.isMacCatalyst}`],
    ["Platform.select", Platform.select({ ios: "ios branch", android: "android branch" }) ?? ""],
    ["PixelRatio.get()", String(PixelRatio.get())],
    ["roundToNearestPixel(10.3)", String(PixelRatio.roundToNearestPixel(10.3))],
    ["StyleSheet.hairlineWidth", StyleSheet.hairlineWidth.toFixed(3)],
    ["screenReaderEnabled()", String(screenReaderEnabled())],
    ["reduceMotion()", String(reduceMotion())],
  ];
  const [announced, setAnnounced] = createSignal(0);
  let target: any;
  return (
    <Section title="Platform, PixelRatio, StyleSheet, AccessibilityInfo">
      <For each={rows()}>
        {([k, v]) => (
          <view style={[styles.row, { justifyContent: "space-between" }]}>
            <text style={styles.muted}>{k}</text>
            <text style={styles.label}>{v}</text>
          </view>
        )}
      </For>
      <view style={{ height: StyleSheet.hairlineWidth, backgroundColor: colors.text }} />
      <text style={styles.muted}>The line above is one device pixel tall.</text>
      <view style={styles.row}>
        <UiButton
          label="Announce"
          onPress={() => {
            setAnnounced(announced() + 1);
            AccessibilityInfo.announceForAccessibility(`Announcement ${announced()}`);
          }}
        />
        <UiButton label="Focus the note" color="#57606a" onPress={() => AccessibilityInfo.setAccessibilityFocus(target)} />
      </view>
      <text ref={target} style={styles.muted}>
        With VoiceOver or TalkBack on, the buttons above speak and move the reader here.
      </text>
    </Section>
  );
}

export function Components() {
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }} keyboardDismissMode="interactive">
      <view style={{ padding: 16, gap: 20 }}>
        <Touchables />
        <PressableSheet />
        <LayoutAnimations />
        <Background />
        <Drawer />
        <Accessory />
        <Section title="KeyboardAvoidingView">
          <UiButton label="padding / height / position" onPress={() => nav.push({ name: "page", page: AvoidingPage })} />
        </Section>
        <ModalOptions />
        <PlatformInfo />
      </view>
    </scroll>
  );
}
