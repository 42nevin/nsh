import { mount, signal, View, Scroll, Text, nativeComponent } from 'nsh';
import { Input } from 'nsh/base';
const NativeAction = nativeComponent('mixed.action', { props: ['id', 'title', 'enabled'], events: ['press'] });
const Bridge = nativeComponent('mixed.bridge', { events: ['control'] });
const caseName = '__CASE__';
const phase = signal('__INITIAL_PHASE__');
const merged = signal(false), hidden = signal(false), enabled = signal(true);
const title = signal(false), listening = signal(true), assistiveOnly = signal(false), longOnly = signal(false), bothHandlers = signal(false), noise = signal('Unrelated initial text'), input = signal('Seed value');
const rows = signal([{ id: 'row-a' }, { id: 'row-b' }]), present = signal(true);
let bridge;
let nextRef = 0;
const refs = {}, nodes = new WeakMap(), sequences = {};
function ref(id, node) {
  if (node) { if (!nodes.has(node)) nodes.set(node, ++nextRef); refs[id] = nodes.get(node); }
  else delete refs[id];
}
// Application-owned interface; appearance selects rendering, never source admission.
function Action(props) {
  const appearance = props.appearance;
  const deliver = raw => {
    bridge.command('facade_entry', { id: props.id, appearance, phase: phase(), enabled: props.enabled, ref: refs[props.id] });
    if (!props.enabled) return;
    const sequence = (sequences[props.id] || 0) + 1;
    sequences[props.id] = sequence;
    props.onInvoke({ id: props.id, appearance, phase: phase(), sequence, ref: refs[props.id], raw: raw || null });
  };
  if (appearance === 'native') return <NativeAction id={props.id} title={props.title} enabled={props.enabled}
    pointerEvents={props.assistiveOnly ? 'none' : undefined} focusable={props.assistiveOnly ? false : undefined}
    ref={node => { ref(props.id, node); node?.command('bind_ref', { id: props.id, ref: refs[props.id] }); }} onPress={deliver} style={{ width: 220, height: 32 }} />;
  return <View ref={node => ref(props.id, node)} testID={props.id}
    accessibilityLabel={props.title} role="button" accessibilityState={{ disabled: !props.enabled }}
    pointerEvents={props.assistiveOnly ? 'none' : undefined} focusable={props.assistiveOnly ? false : undefined}
    disabled={!props.enabled} onPress={props.listening === false || props.longOnly ? undefined : () => deliver(null)}
    onLongPress={props.longOnly || props.bothHandlers ? () => deliver({ source: 'longPress' }) : undefined}
    style={{ width: 220, height: 32 }}>
      <Text>{props.title}</Text>
    </View>;
}
const callback = payload => bridge.command('callback', payload);
function control(payload) {
  phase.set(payload.phase);
  if (payload.phase === 'intermediate') { if (caseName === 'mixed-merge') merged.set(true); else hidden.set(true); }
  if (payload.phase === 'restored') { merged.set(false); hidden.set(false); }
  if (payload.phase === 'unrelated') noise.set('Unrelated changed text');
  if (payload.phase === 'updated') { title.set(true); enabled.set(false); input.set('Updated value'); }
  if (payload.phase === 'enabled') enabled.set(true);
  if (payload.phase === 'reordered') rows.set([{ id: 'row-b' }, { id: 'row-a' }]);
  if (payload.phase === 'conditional-off') present.set(false);
  if (payload.phase === 'conditional-on') present.set(true);
  if (payload.phase === 'listener-off') listening.set(false);
  if (payload.phase === 'listener-on') listening.set(true);
  if (payload.phase === 'assistive-only') assistiveOnly.set(true);
  if (payload.phase === 'long-only') longOnly.set(true);
  if (payload.phase === 'both-handlers') { longOnly.set(false); bothHandlers.set(true); }
  bridge.command('mode_receipt', { ...payload, refs: { ...refs }, js_state: {
    merged: merged(), hidden: hidden(), enabled: enabled(), input: input(),
    rows: rows().map(row => row.id), present: present(), listening: listening(), assistiveOnly: assistiveOnly(), longOnly: longOnly(), bothHandlers: bothHandlers(), noise: noise() } });
}
mount(() => <View style={{ flex: 1 }}><Scroll style={{ flex: 1 }}><View style={{ gap: 4 }}>
  <Action id="mixed.sibling" appearance="native" title="Live sibling" enabled={true} onInvoke={callback} />
  <View testID="mixed.group" accessible={merged()} accessibilityLabel="Mixed group"
    pointerEvents={assistiveOnly() ? 'none' : undefined}
    accessibilityElementsHidden={hidden()} style={{ gap: 4 }}>
    <Text testID="mixed.text" accessibilityLabel="Mixed text">{noise()}</Text>
    <Input testID="mixed.input" accessibilityLabel="Mixed input" value={input()} style={{ width: 220, height: 32 }} />
    <Action id="mixed.native" appearance="native" title={title() ? 'Updated native' : 'Native action'} enabled={enabled()} assistiveOnly={assistiveOnly()} onInvoke={callback} />
    <Action id="mixed.builtin" appearance="custom" title={title() ? 'Updated built-in' : 'Built-in action'} enabled={enabled()} listening={listening()} assistiveOnly={assistiveOnly()} longOnly={longOnly()} bothHandlers={bothHandlers()} onInvoke={callback} />
    <Action id="mixed.disabled-native" appearance="native" title="Disabled native" enabled={false} onInvoke={callback} />
    <Action id="mixed.disabled-builtin" appearance="custom" title="Disabled built-in" enabled={false} onInvoke={callback} />
    <If when={caseName === 'mixed-reactive'}>
      <Each of={rows()} key={row => row.id}>{row => <Action id={'mixed.' + row().id} appearance="native"
        title={'Keyed ' + row().id} enabled={true} onInvoke={callback} />}</Each>
      <If when={present()}><Action id="mixed.conditional" appearance="native" title="Conditional native" enabled={true} onInvoke={callback} /></If>
    </If>
  </View>
  <Bridge style={{ width: 1, height: 1 }} ref={node => { bridge = node; node?.command('ready', { case: caseName, refs: { ...refs } }); }} onControl={control} />
</View></Scroll></View>);
