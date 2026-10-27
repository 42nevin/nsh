//! Application-menu choices while the platform still owns an older menu.
//!
//! The mount sink deliberately retains each described menu, like a platform
//! whose UI thread has not applied its next batch yet. Its selections must
//! keep their original meaning instead of being lost or retargeted.

use std::{sync::Arc, time::Duration};

use crossbeam_channel::Receiver;
use nsh_core::{
    Command, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID,
    desktop::{DesktopCommand, MenuItem},
    keymap::{KeyMatch, Keymap, Keystroke},
    platform::Os,
};
use nsh_engine::{Engine, Environment};

use super::components::start_with;

const APP: &str = r#"import { mount, signal, Text, keymap } from "nsh";
import { Menu } from "nsh/desktop";

const chosen = signal("");
const phase = signal("ready");
function install(name: string) {
  Menu.setApplicationMenu([{ label: "Actions", submenu: [
    { label: name, onSelect: () => chosen.set(`${chosen()}${name},`) },
  ] }]);
}
install("old");
keymap.bind("alt+n", () => {
  install("middle");
  install("new");
  phase.set("replaced");
});
keymap.bind("alt+c", () => {
  Menu.setApplicationMenu(null);
  phase.set("cleared");
});
keymap.bind("alt+i", () => {
  try {
    Menu.setApplicationMenu([{ label: "Invalid", shortcut: "mod++", onSelect: () => chosen.set("invalid") }]);
  } catch {
    phase.set("invalid rejected");
  }
});
keymap.bind("alt+b", () => phase.set(`barrier ${chosen()}`));
mount(() => <><Text>{`phase ${phase()}`}</Text><Text>{`chosen ${chosen()}`}</Text></>);
"#;

#[derive(Default)]
struct Seen {
    menus: Vec<Option<Arc<[MenuItem]>>>,
    keys: Option<Arc<Keymap>>,
    texts: Vec<String>,
}

fn next(rx: &Receiver<MountBatch>, wanted: &str) -> Seen {
    let mut seen = Seen::default();
    while let Ok(batch) = rx.recv_timeout(Duration::from_secs(5)) {
        for item in batch.items {
            match item {
                MountItem::Text { text, .. } => seen.texts.push(text.text.to_string()),
                MountItem::Keymap(keys) => seen.keys = Some(keys),
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Desktop(command),
                } => {
                    if let DesktopCommand::SetMenu(items) = *command {
                        seen.menus.push(items);
                    }
                }
                _ => {}
            }
        }
        if seen.texts.iter().any(|text| text == wanted) {
            return seen;
        }
    }
    panic!("expected {wanted:?}; received {:?}", seen.texts);
}

fn start() -> (Engine, Receiver<MountBatch>, Seen) {
    let (engine, rx) = start_with(
        APP,
        Environment {
            os: Os::MacOs,
            ..Environment::default()
        },
        None,
        None,
    );
    let seen = next(&rx, "phase ready");
    (engine, rx, seen)
}

fn press(engine: &Engine, keys: &Keymap, key: &str) {
    let key = Keystroke::parse(key, Os::MacOs).unwrap();
    let KeyMatch::Fire(id) = keys.find(key, false, false) else {
        panic!("the fixture did not bind {key:?}");
    };
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::KeyCommand,
        EventPayload::Value(PayloadValue::Number(id as f64)),
    );
}

fn choose(engine: &Engine, menu: &[MenuItem]) {
    let mut ids = Vec::new();
    MenuItem::each_action(menu, &mut |action| ids.push(action.id));
    assert_eq!(ids.len(), 1, "one unambiguous native choice");
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![("id", PayloadValue::Number(ids[0] as f64))]),
    );
}

#[test]
fn current_and_in_flight_menu_choices_keep_their_callbacks() {
    let (engine, rx, initial) = start();
    let old = initial.menus[0].as_ref().unwrap();
    press(&engine, initial.keys.as_ref().unwrap(), "alt+n");
    let replacement = next(&rx, "phase replaced");
    assert_eq!(replacement.menus.len(), 2);
    choose(&engine, old);
    choose(&engine, replacement.menus[0].as_ref().unwrap());
    choose(&engine, replacement.menus[1].as_ref().unwrap());
    next(&rx, "chosen old,middle,new,");
}

#[test]
fn clearing_a_menu_keeps_its_already_available_choice() {
    let (engine, rx, initial) = start();
    press(&engine, initial.keys.as_ref().unwrap(), "alt+c");
    let replacement = next(&rx, "phase cleared");
    assert!(matches!(replacement.menus.as_slice(), [None]));
    choose(&engine, initial.menus[0].as_ref().unwrap());
    next(&rx, "chosen old,");
}

#[test]
fn invalid_replacement_keeps_the_installed_menu() {
    let (engine, rx, initial) = start();
    press(&engine, initial.keys.as_ref().unwrap(), "alt+i");
    let replacement = next(&rx, "phase invalid rejected");
    assert!(replacement.menus.is_empty());
    choose(&engine, initial.menus[0].as_ref().unwrap());
    next(&rx, "chosen old,");
}

#[test]
fn a_choice_before_the_last_native_owner_drops_is_delivered_once() {
    let (engine, rx, initial) = start();
    let old = initial.menus[0].as_ref().unwrap();
    let native = engine.control().retain_menu_callbacks(old);
    let tracking = native.clone();
    press(&engine, initial.keys.as_ref().unwrap(), "alt+c");
    next(&rx, "phase cleared");
    drop(native);
    choose(&engine, old);
    drop(tracking);
    // This stale ID is deliberately injected after native ownership ended.
    // A barrier on the same input stream distinguishes rejection from delay.
    choose(&engine, old);
    press(&engine, initial.keys.as_ref().unwrap(), "alt+b");
    next(&rx, "phase barrier old,");
}

#[test]
fn retiring_one_native_menu_does_not_retire_older_or_newer_owners() {
    let (engine, rx, initial) = start();
    let old = initial.menus[0].as_ref().unwrap();
    let old_owner = engine.control().retain_menu_callbacks(old);
    press(&engine, initial.keys.as_ref().unwrap(), "alt+n");
    let replacement = next(&rx, "phase replaced");
    let middle = replacement.menus[0].as_ref().unwrap();
    let newest = replacement.menus[1].as_ref().unwrap();
    let middle_owner = engine.control().retain_menu_callbacks(middle);
    let newest_owner = engine.control().retain_menu_callbacks(newest);
    drop(middle_owner);
    choose(&engine, middle);
    choose(&engine, old);
    choose(&engine, newest);
    press(&engine, initial.keys.as_ref().unwrap(), "alt+b");
    next(&rx, "phase barrier old,new,");
    drop(old_owner);
    drop(newest_owner);
    choose(&engine, old);
    choose(&engine, newest);
    press(&engine, initial.keys.as_ref().unwrap(), "alt+c");
    let last = next(&rx, "phase cleared");
    assert!(
        !last.texts.iter().any(|text| text.starts_with("chosen ")),
        "no retired callback ran before the ordered clear barrier: {:?}",
        last.texts
    );
}
