import { Text, View, signal } from "nsh";
import { Calendar, type CalendarValue } from "nsh/base";

export default function BaseCalendar() {
  const month = signal("2030-12-01");
  const value = signal<CalendarValue>("2040-03-01");
  return <View>
    <Text>{`Month ${month()}`}</Text>
    <Text>{`Selected ${value()}`}</Text>
    <Calendar value={value()} month={month()} today="2030-12-31" interactiveCaption={false}
      dayLabels={{ "2030-12-31": "Year end" }}
      onMonthChange={(e) => month.set(e.month)} onChange={(e) => value.set(e.value)}
      previousAccessibilityLabel="Return" nextAccessibilityLabel="Forward"
      renderItem={(i) => <Text>{i.today ? `Current ${i.date}` : String(i.value)}</Text>} />
  </View>;
}
