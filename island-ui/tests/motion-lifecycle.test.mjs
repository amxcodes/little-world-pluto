import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../src/main.jsx', import.meta.url), 'utf8');

test('playback clock stops while hidden and releases its timer on unmount', () => {
  const start = source.indexOf('function MediaTransport(');
  const code = source.slice(start, source.indexOf('  return <>', start)) + '\n}';
  const effects = [], intervals = new Map(), listeners = new Map();
  let nextTimer = 0;
  const document = { hidden: false, addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name) };
  const context = {
    document, performance: { now: () => 1000 },
    useState: value => [typeof value === 'function' ? value() : value, () => {}],
    useRef: value => ({ current: value }), useEffect: fn => effects.push(fn),
    setInterval: (fn, delay) => { const id = ++nextTimer; intervals.set(id, { fn, delay }); return id; },
    clearInterval: id => intervals.delete(id), clearTimeout() {}, setTimeout() {}, send() {},
  };
  vm.runInNewContext(code + '\nMediaTransport({ media: { playing: true }, receivedAt: 1000 });', context);
  const cleanups = effects.map(fn => fn()).filter(fn => typeof fn === 'function');
  assert.equal(intervals.size, 1);
  assert.equal([...intervals.values()][0].delay, 250);
  document.hidden = true; listeners.get('visibilitychange')();
  assert.equal(intervals.size, 0);
  document.hidden = false; listeners.get('visibilitychange')();
  assert.equal(intervals.size, 1);
  cleanups.forEach(fn => fn());
  assert.equal(intervals.size, 0);
  assert.equal(listeners.size, 0);
});

test('Ask releases an owned Space press even after focus moves to an editor', () => {
  const start = source.indexOf('  const askCaptureState =');
  const code = source.slice(start, source.indexOf('  const [homeActions', start));
  const listeners = new Map(), sent = [], events = [];
  let layout, effect;
  const context = {
    state: { agents: { opencode: { connected: true, sessions: [] } }, speechSettings: { configured: true } },
    dictateOpen: false, displayPage: 'Ask', settingsOpen: false,
    useRef: () => ({ current: null }), useLayoutEffect: fn => { layout = fn; }, useEffect: fn => { effect = fn; },
    document: { activeElement: null, querySelector: () => null },
    window: { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name), dispatchEvent: event => events.push(event.type) },
    Event, isEditor: target => target === 'editor', readAgentPreferences: () => ({ defaultProvider: 'opencode' }),
    send: name => sent.push(name), setHoldCapture() {}, setAgentDictateAutoSubmit() {}, setDictateMode() {}, setDictateOpen() {},
  };
  vm.runInNewContext(code, context); layout(); const cleanup = effect();
  const event = target => ({ code: 'Space', target, preventDefault() {}, stopImmediatePropagation() {} });
  listeners.get('keydown')(event('button'));
  context.dictateOpen = true; layout(); // A render updates the ref without rebinding listeners.
  listeners.get('keyup')(event('editor'));
  assert.deepEqual(sent, ['holdSpaceBegin', 'holdSpaceEnd']);
  assert.deepEqual(events, ['island:hold-release']);
  cleanup(); assert.equal(listeners.size, 0);
});

test('Home Dictate clears the previous Ask hold state before opening', () => {
  const start = source.indexOf('  const openDictation =');
  const code = source.slice(start, source.indexOf('  const openSignature', start));
  let held = true, opened = false;
  const context = { state: {}, window: { islandHoldReleased: true }, setHoldCapture: value => { held = value; }, setDictateOpen: value => { opened = value; } };
  for (const name of ['setAgentDictateAutoSubmit', 'setSignOpen', 'setScreenResult', 'setCommandOpen', 'setSettingsOpen', 'setScreenSelection', 'setDictateMode', 'setDictateTranscriptExpanded']) context[name] = () => {};
  vm.runInNewContext(code + '\nopenDictation();', context);
  assert.equal(held, false); assert.equal(opened, true); assert.equal(context.window.islandHoldReleased, false);
});

test('layout observer ignores streamed descendants but reports preferred-height changes', () => {
  const start = source.indexOf('  useLayoutEffect(() => {\n    // The profile editor');
  const code = source.slice(start, source.indexOf('  const clips =', start));
  let effect, mutate;
  const frames = new Map(), sends = [], root = { dataset: { layoutHeight: '330' }, style: {} };
  const content = { firstElementChild: root, style: {}, scrollHeight: 330 };
  const context = {
    useLayoutEffect: fn => { effect = fn; },
    state: { expanded: true, compactRenderGeneration: 1 }, displayPage: 'Agents', settingsOpen: false,
    bodyRef: { current: { firstElementChild: content } }, layoutWideRef: { current: false },
    document: { querySelector: () => ({ offsetHeight: 43 }) }, getComputedStyle: () => ({ paddingTop: '12', paddingBottom: '12' }),
    ResizeObserver: class { observe() {} unobserve() {} disconnect() {} },
    MutationObserver: class { constructor(fn) { mutate = fn; } observe() {} disconnect() {} },
    requestAnimationFrame: fn => { frames.set(1, fn); return 1; }, cancelAnimationFrame: id => frames.delete(id),
    send: (kind, data) => sends.push({ kind, ...data }), window: {},
  };
  for (const name of ['noteView', 'showReminderCalendar', 'signOpen', 'showFolderInput', 'folderEditing', 'dictateOpen', 'dictateMode', 'dictateTranscriptExpanded', 'screenSelection', 'screenResult', 'commandOpen']) context[name] = false;
  vm.runInNewContext(code, context); const cleanup = effect();
  const paint = () => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(fn => fn()); };
  paint(); assert.equal(sends.length, 1);
  for (let i = 0; i < 100; i++) mutate([{ type: 'childList', target: root }]);
  assert.equal(frames.size, 0);
  root.dataset.layoutHeight = '90'; mutate([{ type: 'attributes', target: root }]); paint();
  assert.equal(sends.length, 2); assert.equal(sends[1].height, 157);
  cleanup(); assert.equal(context.window.islandRequestLayout, undefined);
  context.state.wakeWord = { surface: true, revision: 8 };
  context.document.querySelector = () => ({ offsetHeight: 0 });
  root.dataset.layoutHeight = '34';
  vm.runInNewContext(code, context); const cleanupWake = effect(); paint();
  assert.equal(sends.at(-1).height + 32, 90); // Native inset is added exactly once.
  assert.equal(sends.at(-1).wakeSurface, true);
  assert.equal(sends.at(-1).wakeRevision, 8);
  root.querySelector = () => ({ dataset: { layoutHeight: '138' } });
  mutate([{ type: 'attributes', target: root }]); paint();
  assert.equal(sends.at(-1).height + 32, 194); // Capture errors remain readable.
  cleanupWake();
});

test('rapid conversion batches retain old delays and start new rows at zero', () => {
  const conversion = readFileSync(new URL('../src/conversion-ui.jsx', import.meta.url), 'utf8');
  const start = conversion.indexOf('  const seenItems =');
  const code = conversion.slice(start, conversion.indexOf('  const seenReadyItems', start));
  const seen = { current: new Map() };
  let effect;
  const context = { useRef: () => seen, useMemo: fn => fn(), useEffect: fn => { effect = fn; } };
  vm.createContext(context);
  vm.runInContext('function derive(items) {\n' + code + '\nreturn arrivalOrder; }', context);
  const batch = ids => {
    context.items = ids.map(id => ({ id }));
    const result = vm.runInContext('derive(items)', context);
    effect();
    return result;
  };
  const first = batch(['a', 'b']);
  assert.equal(first.get('a'), 0); assert.equal(first.get('b'), 1);
  const next = batch(['a', 'b', 'c', 'd']);
  assert.equal(next.get('b'), 1); assert.equal(next.get('c'), 0); assert.equal(next.get('d'), 1);
  batch(['c', 'd']); assert.equal(seen.current.size, 2);
});
