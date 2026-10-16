// TextField: labels, placeholders, controlled and uncontrolled text, secure entry.
import { mount, signal } from "nsh";
import { Button, TextField } from "nsh/ui/mobile";

function App() {
  const name = signal("Ada");
  const submitted = signal("");
  const locked = signal(false);
  return (
    <view style={{ flex: 1, backgroundColor: "#ffffff", padding: 16, paddingTop: 24, gap: 16 }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>TextField</text>
      <TextField
        label="Name (controlled)"
        placeholder="Your name"
        value={name()}
        onChange={(v) => name.set(v)}
        onSubmit={(v) => submitted.set(v)}
        disabled={locked()}
      />
      <text style={{ color: "#737373" }}>{`Hello, ${name() || "nobody"}`}</text>
      <TextField label="Password" placeholder="At least 8 characters" secure />
      <TextField label="Notes (no clear button)" defaultValue="Uncontrolled" clearable={false} />
      <Button
        variant="secondary"
        title={locked() ? "Enable the name" : "Disable the name"}
        onPress={() => locked.set(!locked())}
      />
      <text style={{ color: "#737373" }}>{submitted() ? `Submitted: ${submitted()}` : "Press return to submit."}</text>
    </view>
  );
}

mount(App);
