import { mount, signal, type YNode } from "nsh";

const field = {
  fontSize: 16,
  color: "#1f2328",
  backgroundColor: "#ffffff",
  borderWidth: 1,
  borderColor: "#d0d7de",
  borderRadius: 10,
  paddingHorizontal: 12,
  paddingVertical: 10,
} as const;
const label = { fontSize: 13, color: "#57606a" } as const;

function App() {
  const name = signal("");
  const email = signal("");
  const code = signal("");
  const notes = signal("");
  const notesHeight = signal(0);
  const focused = signal("nothing");
  const sent = signal("");
  let emailInput!: YNode;
  let passwordInput!: YNode;
  let notesInput!: YNode;

  let tries = 0;
  const submit = () => sent.set(`#${++tries}: signed up ${name() || "nobody"} <${email() || "no e-mail"}>`);

  return (
    <scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }} keyboardDismissMode="on-drag" keyboardShouldPersistTaps="handled">
      <view style={{ padding: 16, paddingTop: 24, gap: 12 }}>
        <text style={{ fontSize: 22, fontWeight: "700" }}>Sign up</text>
        <text style={label}>Focused: {focused()}</text>

        <text style={label}>Name (controlled, return goes to the next field)</text>
        <input
          style={field}
          value={name()}
          onChangeText={(t) => name.set(t)}
          placeholder="Ada Lovelace"
          autoCapitalize="words"
          autoComplete="name"
          returnKeyType="next"
          blurOnSubmit={false}
          onSubmitEditing={() => emailInput.focus()}
          onFocus={() => focused.set("name")}
        />

        <text style={label}>E-mail</text>
        <input
          ref={emailInput}
          style={field}
          value={email()}
          onChangeText={(t) => email.set(t.trim().toLowerCase())}
          placeholder="ada@example.com"
          keyboardType="email-address"
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="email"
          returnKeyType="next"
          blurOnSubmit={false}
          onSubmitEditing={() => passwordInput.focus()}
          onFocus={() => focused.set("e-mail")}
        />

        <text style={label}>Password (uncontrolled)</text>
        <input
          ref={passwordInput}
          style={field}
          secureTextEntry
          autoComplete="new-password"
          placeholder="At least 8 characters"
          returnKeyType="next"
          blurOnSubmit={false}
          onSubmitEditing={() => notesInput.focus()}
          onFocus={() => focused.set("password")}
        />

        <text style={label}>Code (number pad, 6 digits)</text>
        <input
          style={{ ...field, letterSpacing: 8, textAlign: "center", fontSize: 22 }}
          value={code()}
          onChangeText={(t) => code.set(t.replace(/[^0-9]/g, ""))}
          keyboardType="number-pad"
          autoComplete="one-time-code"
          maxLength={6}
          onFocus={() => focused.set("code")}
        />

        <text style={label}>About you (grows with its text, 200 max)</text>
        <input
          ref={notesInput}
          multiline
          maxLength={200}
          value={notes()}
          onChangeText={(t) => notes.set(t)}
          onContentSizeChange={(e) => notesHeight.set(Math.min(e.height, 120))}
          placeholder="Return adds a line"
          style={{ ...field, height: Math.max(notesHeight(), 22) + 20 }}
          onFocus={() => focused.set("notes")}
          onBlur={() => focused.set("nothing")}
        />
        <text style={{ ...label, textAlign: "right" }}>{`${notes().length}/200`}</text>

        <view
          onPress={submit}
          activeOpacity={0.6}
          style={{ padding: 14, borderRadius: 10, backgroundColor: "#0969da", alignItems: "center" }}
        >
          <text style={{ color: "#ffffff", fontWeight: "600" }}>Sign up</text>
        </view>
        <text style={{ color: "#1a7f37" }}>{sent()}</text>
      </view>
    </scroll>
  );
}

mount(App);
