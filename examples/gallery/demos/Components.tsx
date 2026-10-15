import { createSignal, For, Show } from "solid-js";
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";
import { LayoutAnimation } from "nsh/animated";
import { PixelRatio, Platform } from "nsh/device";
import { InputAccessoryView } from "nsh/ios";
import { KeyboardAvoidingView } from "nsh/keyboard";
import { Modal, Screen } from "nsh/navigation";
import { SafeAreaView } from "nsh/safe-area";
import { nav } from "../nav";
import { Button as UiButton, colors, Section } from "../ui";

const hairline = 1 / PixelRatio.get();

const styles = {
  row: { flexDirection: "row", alignItems: "center", gap: 12 },
  card: { backgroundColor: "#ffffff", borderRadius: 10, padding: 12, borderWidth: hairline, borderColor: colors.border },
  label: { fontSize: 15, color: colors.text },
  muted: { fontSize: 13, color: colors.muted },
} as const;

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
    <Section title="Sheet">
      <view
        onPress={() => setSheet(true)}
        class="rounded-[10px] bg-[#8250df] p-3.5 transition active:scale-[0.97] active:bg-[#bf3989]"
      >
        <text style={{ color: "#ffffff", fontWeight: "600" }}>Open a sheet</text>
      </view>
      <Modal visible={sheet()} ios={{ presentationStyle: "pageSheet" }} animationType="slide" onRequestClose={() => setSheet(false)}>
        <SafeAreaView style={{ flex: 1, backgroundColor: colors.surface, padding: 20, gap: 16 }}>
          <text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</text>
          <text style={{ color: colors.muted }}>Swipe down (iOS) or press back (Android) to close.</text>
          <UiButton label="Close" onPress={() => setSheet(false)} />
        </SafeAreaView>
      </Modal>
    </Section>
  );
}

/** Press feedback is native: opacity, an underlay, classes; `disabled` takes no presses. */
function PressFeedback() {
  const [last, setLast] = createSignal("nothing yet");
  const [disabled, setDisabled] = createSignal(false);
  return (
    <Section title="Press feedback">
      <view style={styles.card} activeOpacity={0.4} onPress={() => setLast("activeOpacity")}>
        <text style={styles.label}>activeOpacity: fades while pressed</text>
      </view>
      <view style={styles.card} underlayColor="#dbeafe" onPress={() => setLast("underlayColor")}>
        <text style={styles.label}>underlayColor: the background while pressed</text>
      </view>
      <view
        style={styles.card}
        disabled={disabled()}
        underlayColor="#fee2e2"
        class="disabled:opacity-40"
        onPress={() => setLast("disabled view")}
      >
        <text style={styles.label}>{disabled() ? "disabled: takes no presses" : "enabled"}</text>
      </view>
      <view style={styles.row}>
        <UiButton label={disabled() ? "Enable" : "Disable"} color="#57606a" onPress={() => setDisabled(!disabled())} />
        <text style={styles.muted}>Last pressed: {last()}</text>
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
        ios={{ inputAccessoryViewID: "counter" }}
        style={{ borderWidth: 1, borderColor: colors.border, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 8, fontSize: 16 }}
      />
      <Show when={Platform.OS === "android"}>
        <text style={styles.muted}>Android has no input accessories.</text>
      </Show>
      <InputAccessoryView nativeID="counter" backgroundColor="#f6f8fa">
        <view style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: 12, paddingVertical: 8, borderTopWidth: hairline, borderTopColor: colors.border }}>
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
        android={{ statusBarTranslucent: translucent() }}
        ios={{ supportedOrientations: ["portrait", "landscape"] }}
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
    ["screenReaderEnabled()", String(screenReaderEnabled())],
    ["reduceMotion()", String(reduceMotion())],
  ];
  const [announced, setAnnounced] = createSignal(0);
  let target: any;
  return (
    <Section title="Platform, PixelRatio, AccessibilityInfo">
      <For each={rows()}>
        {([k, v]) => (
          <view style={[styles.row, { justifyContent: "space-between" }]}>
            <text style={styles.muted}>{k}</text>
            <text style={styles.label}>{v}</text>
          </view>
        )}
      </For>
      <view style={{ height: hairline, backgroundColor: colors.text }} />
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
        <PressFeedback />
        <PressableSheet />
        <LayoutAnimations />
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
