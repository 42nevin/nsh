// Validation sends focus to the public ref, without a prior pointer press on a
// segment. The next hardware key edits the selected segment of the native field.
import { Text, View, signal } from "nsh";
import { TimeField, type TimeFieldRef } from "nsh/base";

export default function App() {
  let field: TimeFieldRef;
  const time = signal("06:30");
  return <View>
    <View testID="focus-hour" onPress={() => field.focus()}><Text>Focus hour</Text></View>
    <TimeField ref={node => { field = node; }} value={time()} onChange={e => time.set(e.value)}
      accessibilityLabel="Hour" segmentClass="px-1 focus:opacity-50"/>
    <Text>{`Edited: ${time()}`}</Text>
  </View>;
}
