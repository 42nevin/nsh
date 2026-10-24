import { mount, View } from "nsh";
import { nativeComponent } from "nsh/native";
import { Menu } from "nsh/desktop";

const Probe = nativeComponent("MenuLifecycleProbe", {
  props: [],
  events: ["replace", "install", "clear", "invalid", "barrier", "burst", "collect"],
});
let probe: any;
let sequence = 0;
const resources: WeakRef<object>[] = [];
function install(name: string) {
  Menu.setApplicationMenu([{ label: "Actions", submenu: [
    { label: "Choose", onSelect: () => probe.command(name) },
    { label: "Disabled", disabled: true, onSelect: () => probe.command("forbidden") },
  ] }]);
}
install("old");
mount(() => <View><Probe
  ref={node => { probe = node; node?.command("ready"); }}
  onReplace={() => {
    probe.command("choose-current");
    install(`new-${++sequence}`);
    probe.command("installed");
  }}
  onInstall={() => {
    install(`new-${++sequence}`);
    probe.command("installed");
  }}
  onClear={() => {
    probe.command("choose-current");
    Menu.setApplicationMenu(null);
    probe.command("cleared");
  }}
  onInvalid={() => {
    try {
      Menu.setApplicationMenu([{ label: "Invalid", shortcut: "mod++", onSelect: () => probe.command("invalid-action") }]);
    } catch {
      probe.command("invalid-rejected");
    }
  }}
  onBarrier={() => probe.command("barrier")}
  onBurst={() => {
    for (let i = 0; i < 64; i++) {
      const marker: any = {};
      const callback = () => marker;
      marker.callback = callback;
      resources.push(new WeakRef(marker));
      Menu.setApplicationMenu([{ label: "Actions", submenu: [
        { label: "Collectible", onSelect: callback },
      ] }]);
    }
    Menu.setApplicationMenu(null);
    probe.command("burst-cleared");
  }}
  onCollect={() => {
    // Use the engine's normal idle GC, not a test-only retirement/GC hook.
    setTimeout(() => {
      const alive = resources.filter(ref => ref.deref() !== undefined).length;
      probe.command(`collected-${alive}`);
    }, 2000);
  }}
/></View>);
