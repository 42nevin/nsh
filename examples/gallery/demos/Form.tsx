import { createSignal, For, splitProps } from "solid-js";
import { type InputProps, type YNode } from "nsh";
import { Keyboard, keyboard, KeyboardAvoidingView } from "nsh/keyboard";
import { Modal, Screen } from "nsh/navigation";
import { SafeAreaView, useSafeAreaInsets } from "nsh/safe-area";
import { nav } from "../nav";
import { Button, colors, Section } from "../ui";

const field = {
  fontSize: 16,
  color: colors.text,
  backgroundColor: colors.surface,
  borderWidth: 1,
  borderColor: colors.border,
  borderRadius: 10,
  paddingHorizontal: 12,
  paddingVertical: 10,
} as const;

/** A labelled input; `next` focuses another input on return. */
function Field(props: InputProps & { label: string; next?: () => YNode | undefined }) {
  const [own, input] = splitProps(props, ["label", "next"]);
  return (
    <view style={{ gap: 4 }}>
      <text style={{ fontSize: 13, color: colors.muted }}>{own.label}</text>
      <input
        style={field}
        returnKeyType={own.next ? "next" : "done"}
        onSubmitEditing={() => own.next?.()?.focus()}
        {...input}
      />
    </view>
  );
}

type Mode = "none" | "on-drag" | "interactive";

/**
 * Many inputs in a scroll view: the focused one stays above the keyboard
 * (natively), return moves to the next one, dragging the form can hide the
 * keyboard.
 */
export function Form() {
  const [name, setName] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [shout, setShout] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [mode, setMode] = createSignal<Mode>("on-drag");
  const [submitted, setSubmitted] = createSignal("");
  const [sheet, setSheet] = createSignal(false);
  // Inputs in return-key order.
  const refs: YNode[] = [];
  const next = (i: number) => () => refs[i + 1];
  const ref = (i: number) => (node: YNode) => (refs[i] = node);

  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }} keyboardDismissMode={mode()}>
      <view style={{ padding: 16, gap: 20 }}>
        <view style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
          <text style={{ flex: 1, color: colors.muted }}>Keyboard {Math.round(keyboard().height)} pt</text>
          <For each={["none", "on-drag", "interactive"] as Mode[]}>
            {(m) => (
              <text
                onPress={() => setMode(m)}
                style={{
                  fontSize: 13,
                  paddingHorizontal: 8,
                  paddingVertical: 4,
                  borderRadius: 6,
                  overflow: "hidden",
                  color: mode() === m ? "#ffffff" : colors.text,
                  backgroundColor: mode() === m ? colors.accent : "#e1e4e8",
                }}
              >
                {m}
              </text>
            )}
          </For>
        </view>

        <Section title="Account">
          <Field ref={ref(0)} next={next(0)} label="Name" value={name()} onChangeText={setName} autoCapitalize="words" autoComplete="name" placeholder="Ada Lovelace" />
          <Field
            ref={ref(1)}
            next={next(1)}
            label="E-mail"
            value={email()}
            onChangeText={setEmail}
            keyboardType="email-address"
            autoCapitalize="none"
            autoCorrect={false}
            autoComplete="email"
            placeholder="ada@example.com"
          />
          <Field ref={ref(2)} next={next(2)} label="Password" secureTextEntry autoComplete="new-password" placeholder="At least 8 characters" />
          <Field ref={ref(3)} next={next(3)} label="Phone" keyboardType="phone-pad" autoComplete="tel" placeholder="+1 555 0100" />
          <Field ref={ref(4)} next={next(4)} label="Website" keyboardType="url" autoCapitalize="none" autoCorrect={false} placeholder="https://" />
        </Section>

        <Section title="Numbers">
          <Field ref={ref(5)} next={next(5)} label="Age (number pad, 3 digits)" keyboardType="number-pad" maxLength={3} />
          <Field ref={ref(6)} next={next(6)} label="Amount (decimal pad)" keyboardType="decimal-pad" placeholder="0.00" />
          <Field
            ref={ref(7)}
            next={next(7)}
            label="Code (6 digits)"
            keyboardType="numeric"
            maxLength={6}
            style={{ ...field, letterSpacing: 8, fontSize: 22, textAlign: "center" }}
          />
        </Section>

        <Section title="Address (far down: the form scrolls to keep the input visible)">
          <Field ref={ref(8)} next={next(8)} label="Street" placeholder="12 Analytical Way" />
          <Field ref={ref(9)} next={next(9)} label="City" placeholder="London" />
          <Field ref={ref(10)} next={next(10)} label="Postal code" autoCapitalize="characters" placeholder="SW1A 1AA" />
          <Field ref={ref(11)} next={next(11)} label="Country" placeholder="United Kingdom" />
        </Section>

        <Section title="Selection and keys">
          <SelectionDemo />
        </Section>

        <Section title="Text">
          <Field
            ref={ref(12)}
            next={next(12)}
            label="Controlled: uppercased as you type"
            value={shout()}
            onChangeText={(t) => setShout(t.toUpperCase())}
          />
          <Field label="Read only" value="Not editable" editable={false} style={{ ...field, color: colors.muted, backgroundColor: "#f6f8fa" }} />
          <Field
            ref={ref(13)}
            next={next(13)}
            label="Custom colors"
            placeholder="Placeholder in purple"
            placeholderTextColor="#8250df"
            selectionColor="#bf3989"
          />
          <view style={{ gap: 4 }}>
            <text style={{ fontSize: 13, color: colors.muted }}>Notes (multiline, grows, 280 max)</text>
            <input
              ref={ref(14)}
              style={{ ...field, minHeight: 100 }}
              multiline
              maxLength={280}
              value={notes()}
              onChangeText={setNotes}
              placeholder="Return adds a line"
            />
            <text style={{ fontSize: 12, color: colors.muted, textAlign: "right" }}>{`${notes().length}/280`}</text>
          </view>
          <AutoGrow />
          <view style={{ gap: 4 }}>
            <text style={{ fontSize: 13, color: colors.muted }}>caretHidden</text>
            <input style={field} caretHidden defaultValue="No caret here" />
          </view>
        </Section>

        <view style={{ gap: 10 }}>
          <Button label="Submit" onPress={() => setSubmitted(`${name() || "Nobody"} <${email() || "no e-mail"}>`)} />
          <text style={{ color: "#1a7f37" }}>{submitted()}</text>
          <Button label="Hide the keyboard" color="#57606a" onPress={() => Keyboard.dismiss()} />
          <Button label="Inputs in a sheet" color="#8250df" onPress={() => setSheet(true)} />
          <Button
            label="A message bar at the bottom"
            color="#1a7f37"
            onPress={() => nav.push({ name: "page", page: MessageBar })}
          />
        </view>
      </view>

      <Modal visible={sheet()} ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => setSheet(false)}>
        <SafeAreaView style={{ flex: 1, backgroundColor: colors.background }}>
          <scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
            <view style={{ padding: 16, gap: 12 }}>
              <text style={{ fontSize: 22, fontWeight: "700" }}>A sheet</text>
              <Button label="Close" onPress={() => setSheet(false)} />
              <For each={["First", "Second", "Third", "Fourth", "Fifth", "Sixth", "Seventh"]}>
                {(label) => <Field label={label} placeholder={`${label} field`} />}
              </For>
            </view>
          </scroll>
        </SafeAreaView>
      </Modal>
    </scroll>
  );
}

/** An input pinned to the bottom (outside any scroll view): it rides on the keyboard. */
/** A multiline input that grows with its text (onContentSizeChange), up to 5 lines. */
function AutoGrow() {
  const [height, setHeight] = createSignal(0);
  const [size, setSize] = createSignal("");
  return (
    <view style={{ gap: 4 }}>
      <text style={{ fontSize: 13, color: colors.muted }}>Grows with its text (onContentSizeChange): {size()}</text>
      <input
        multiline
        placeholder="Type several lines"
        onContentSizeChange={(e) => {
          setSize(`${Math.round(e.width)}×${Math.round(e.height)}`);
          setHeight(Math.min(e.height, 110));
        }}
        style={{ ...field, height: Math.max(height(), 20) + 20 }}
      />
    </view>
  );
}

function MessageBar() {
  const insets = useSafeAreaInsets();
  const [messages, setMessages] = createSignal<string[]>([]);
  const [draft, setDraft] = createSignal("");
  const send = () => {
    if (!draft().trim()) return;
    setMessages((m) => [...m, draft()]);
    setDraft("");
  };
  return (
    <Screen title="Message bar">
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: colors.background }}>
      <scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        <view style={{ padding: 16, gap: 8 }}>
          <text style={{ color: colors.muted }}>
            The bar below sits outside the scroll view; KeyboardAvoidingView lifts it with the keyboard.
          </text>
          <For each={messages()}>
            {(m) => (
              <text
                style={{
                  alignSelf: "flex-end",
                  color: "#ffffff",
                  backgroundColor: colors.accent,
                  borderRadius: 14,
                  overflow: "hidden",
                  paddingHorizontal: 12,
                  paddingVertical: 8,
                }}
              >
                {m}
              </text>
            )}
          </For>
        </view>
      </scroll>
      <view
        style={{
          flexDirection: "row",
          gap: 8,
          padding: 8,
          // Above the home indicator, unless the keyboard covers it.
          paddingBottom: 8 + (keyboard().height > 0 ? 0 : insets().bottom),
          backgroundColor: colors.surface,
        }}
      >
        <input
          value={draft()}
          onChangeText={setDraft}
          onSubmitEditing={send}
          blurOnSubmit={false}
          placeholder="Message"
          returnKeyType="send"
          style={{ ...field, flex: 1, borderRadius: 18, paddingVertical: 8 }}
        />
        <text onPress={send} style={{ alignSelf: "center", color: colors.accent, fontSize: 16, fontWeight: "600" }}>
          Send
        </text>
      </view>
    </KeyboardAvoidingView>
    </Screen>
  );
}

/** `onSelectionChange`, `onKeyPress`, `selection` and `setSelection`. */
function SelectionDemo() {
  const [text, setText] = createSignal("Select part of this text");
  const [range, setRange] = createSignal({ start: 0, end: 0 });
  const [keys, setKeys] = createSignal<string[]>([]);
  const [selection, setSelection] = createSignal<{ start: number; end?: number } | undefined>(undefined);
  let input: YNode | undefined;
  return (
    <view style={{ gap: 8 }}>
      <input
        ref={(node) => (input = node)}
        value={text()}
        onChangeText={setText}
        selection={selection()}
        onSelectionChange={setRange}
        onKeyPress={(e) => setKeys((k) => [...k, e.key].slice(-8))}
        autoComplete="off"
        style={field}
      />
      <text style={{ fontSize: 13, color: colors.muted }}>
        {`Selection ${range().start}–${range().end}: "${text().slice(range().start, range().end)}"`}
      </text>
      <text style={{ fontSize: 13, color: colors.muted }}>{`Keys: ${keys().join(" ") || "–"}`}</text>
      <view style={{ flexDirection: "row", gap: 8 }}>
        <view style={{ flex: 1 }}>
          <Button label="Select all" onPress={() => setSelection({ start: 0, end: text().length })} />
        </view>
        <view style={{ flex: 1 }}>
          <Button label="Caret to start" color="#57606a" onPress={() => input?.setSelection(0)} />
        </view>
      </view>
      <Field label="One-time code (autofill from messages)" keyboardType="number-pad" autoComplete="one-time-code" maxLength={6} />
    </view>
  );
}
