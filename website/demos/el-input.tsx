import { mount, signal, type YNode } from "nsh";

function App() {
  const name = signal("");
  const password = signal("");
  const notes = signal("");
  const submitted = signal("");
  let passwordInput!: YNode;
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
  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", padding: 20, paddingTop: 24, gap: 10 }}>
      <text style={{ fontSize: 22, fontWeight: "700", color: "#1f2328" }}>input</text>

      <text style={{ color: "#57606a" }}>Name (controlled, maxLength 20)</text>
      <input
        style={field}
        value={name()}
        onChangeText={(text) => name.set(text)}
        placeholder="Ada Lovelace"
        placeholderTextColor="#8c959f"
        autoCapitalize="words"
        maxLength={20}
        returnKeyType="next"
        onSubmitEditing={() => passwordInput.focus()}
      />
      <text style={{ fontSize: 12, color: "#6e7781" }}>{name().length} / 20</text>

      <text style={{ color: "#57606a" }}>Password</text>
      <input
        ref={passwordInput}
        style={field}
        value={password()}
        onChangeText={(text) => password.set(text)}
        secureTextEntry
        autoComplete="current-password"
        returnKeyType="done"
        onSubmitEditing={() => submitted.set(`Signed in as ${name() || "nobody"}`)}
      />

      <text style={{ color: "#57606a" }}>Notes (multiline)</text>
      <input
        style={{ ...field, height: 100 }}
        multiline
        value={notes()}
        onChangeText={(text) => notes.set(text)}
        placeholder="Anything else?"
        placeholderTextColor="#8c959f"
      />

      <view
        onPress={() => submitted.set(`Signed in as ${name() || "nobody"}`)}
        activeOpacity={0.7}
        disabled={password().length === 0}
        class="disabled:opacity-40"
        style={{ backgroundColor: "#1f883d", borderRadius: 10, padding: 14, alignItems: "center", marginTop: 6 }}
      >
        <text style={{ color: "#ffffff", fontSize: 16, fontWeight: "600" }}>Sign in</text>
      </view>
      <text style={{ color: "#1a7f37" }}>{submitted()}</text>
    </view>
  );
}

mount(App);
