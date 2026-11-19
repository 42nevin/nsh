// A link's optional title supplements its destination on pointer devices.
import { Text, View, signal } from "nsh";
import { Link } from "nsh/base";

export default function LinkHover() {
  const shown = signal(true);
  return (
    <View>
      <Link testID="destination" href="https://example.com/docs" title="Documentation reference">
        <Text>Docs</Text>
      </Link>
      <Link testID="fragment" href="#" title="Local destination" showDestinationHint={shown()}
        onOpen={() => shown.set(false)}>
        <Text>Local link</Text>
      </Link>
      <Text>{shown() ? "Hint enabled" : "Hint disabled"}</Text>
    </View>
  );
}
