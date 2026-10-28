import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';

test('display timers stop when hidden, catch up on reveal and clean up', () => {
  const timers = new Map(), listeners = new Map();
  let id = 0, ticks = 0;
  const document = { hidden: true, addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name) };
  const context = vm.createContext({ document, tick: () => ticks++,
    setInterval: (fn, delay) => { timers.set(++id, { fn, delay }); return id; }, clearInterval: key => timers.delete(key) });
  const source = readFileSync(new URL('../src/visible-interval.js', import.meta.url), 'utf8').replace('export function', 'function');
  vm.runInContext(source + '\nvar cleanup = visibleInterval(tick, 1000);', context);
  assert.equal(ticks, 0); assert.equal(timers.size, 0);
  document.hidden = false; listeners.get('visibilitychange')();
  assert.equal(ticks, 1); assert.equal(timers.size, 1);
  listeners.get('visibilitychange')();
  assert.equal(timers.size, 1, 'duplicate reveal must not duplicate timers');
  document.hidden = true; listeners.get('visibilitychange')();
  assert.equal(timers.size, 0);
  document.hidden = false; listeners.get('visibilitychange')();
  assert.equal(ticks, 3, 'reveal immediately catches up to wall time');
  context.cleanup();
  assert.equal(timers.size, 0); assert.equal(listeners.size, 0);
});
