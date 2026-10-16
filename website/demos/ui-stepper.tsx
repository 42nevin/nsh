// Stepper: its own value, a controlled one with a fractional step, and its ref.
import { mount, signal } from "nsh";
import { Button, Group, Stepper, type StepperRef } from "nsh/ui/mobile";

function Row(props: { label: string; children: any }) {
  return (
    <view style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
      <text>{props.label}</text>
      {props.children}
    </view>
  );
}

function App() {
  const quantity = signal(1.5);
  const asked = signal("");
  let own!: StepperRef;
  return (
    <view style={{ flex: 1, backgroundColor: "#f5f5f5", padding: 16, paddingTop: 24, gap: 20 }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>Stepper</text>
      <Group footer={<text>Hold − or + to keep stepping.</text>}>
        <Row label="Own value (0–10)">
          <Stepper ref={own} defaultValue={3} min={0} max={10} />
        </Row>
        <Row label={`Controlled: ${quantity()}`}>
          <Stepper value={quantity()} min={0} max={5} step={0.5} onChange={(v) => quantity.set(v)} />
        </Row>
        <Row label="Disabled">
          <Stepper defaultValue={2} disabled />
        </Row>
      </Group>
      <Button
        variant="secondary"
        title="Read the first, then set it to 7"
        onPress={async () => {
          const was = await own.value();
          own.setValue(7);
          asked.set(`It was ${was}`);
        }}
      />
      <text style={{ color: "#737373" }}>{asked()}</text>
    </view>
  );
}

mount(App);
