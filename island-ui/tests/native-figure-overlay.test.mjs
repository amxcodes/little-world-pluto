import assert from 'node:assert/strict';
import test from 'node:test';
import { Color, Matrix4, PerspectiveCamera } from 'three';
import { createNativeFigureOverlay } from '../src/native-figure-overlay.js';

test('native visibility loss can resume, reuse meshes and dispose callbacks', () => {
  const sent = [], frames = new Map();
  let sequence = 0, changed = 0;
  globalThis.window = new EventTarget();
  window.ipc = { postMessage: text => sent.push(JSON.parse(text)) };
  globalThis.document = new EventTarget();
  document.hidden = false;
  globalThis.innerWidth = 1000;
  globalThis.innerHeight = 800;
  globalThis.requestAnimationFrame = callback => {
    const id = ++sequence;
    frames.set(id, callback);
    return id;
  };
  globalThis.cancelAnimationFrame = id => frames.delete(id);
  const flush = () => {
    const callbacks = [...frames.values()];
    frames.clear();
    callbacks.forEach(callback => callback());
  };
  let historyOpen = false;
  const result = { querySelector: selector => selector === '.agent-turn-history' && historyOpen ? {} : null };
  const canvas = {
    closest: selector => selector === '.agent-result' ? result : null,
    parentElement: null, isConnected: true,
    getBoundingClientRect: () => ({ left: 10, top: 10, width: 100, height: 100, right: 110, bottom: 110 }),
  };
  const mesh = {
    userData: { part: {
      id: 'part', label: 'Selected part', positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
      normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
      indices: new Uint32Array([0, 1, 2]),
      bounds: { center: [0, 0, 0], size: [1, 1, 0] },
    } },
    material: { color: new Color('#aabbcc'), roughness: .5, metalness: 0, opacity: 1 },
    matrixWorld: new Matrix4(),
  };
  const overlay = createNativeFigureOverlay(canvas, () => assert.fail('unexpected native failure'), () => changed++);
  const id = sent[0].id;
  const present = window.islandNativeFigurePresenting;
  overlay.draw([mesh], new PerspectiveCamera(), undefined, 'part');
  flush();
  assert.equal(sent.filter(message => message.kind === 'nativeFigureMeshes').length, 1);
  assert.deepEqual(sent.at(-1).frame.labels, [{ point: [0, 0, 0], text: 'Selected part' }]);
  overlay.draw([mesh], new PerspectiveCamera(), undefined, 'part', [], false);
  flush();
  assert.deepEqual(sent.at(-1).frame.labels, []);
  present(id, true);
  assert.equal(overlay.isPresenting(), true);
  present(id, false);
  assert.equal(overlay.isPresenting(), false);
  assert.equal(changed, 2);
  overlay.draw([mesh], new PerspectiveCamera());
  flush();
  assert.equal(sent.filter(message => message.kind === 'nativeFigureMeshes').length, 1);
  historyOpen = true;
  overlay.draw([mesh], new PerspectiveCamera());
  flush();
  assert.equal(sent.at(-1).kind, 'nativeFigureHide');
  historyOpen = false;
  overlay.draw([mesh], new PerspectiveCamera());
  flush();
  assert.equal(sent.at(-1).kind, 'nativeFigureFrame');
  assert.equal(sent.filter(message => message.kind === 'nativeFigureMeshes').length, 1);
  present(id, true);
  document.hidden = true;
  document.dispatchEvent(new Event('visibilitychange'));
  assert.equal(sent.at(-1).kind, 'nativeFigureHide');
  assert.equal(overlay.isPresenting(), false);
  document.hidden = false;
  document.dispatchEvent(new Event('visibilitychange'));
  flush();
  assert.equal(sent.at(-1).kind, 'nativeFigureFrame');
  overlay.dispose();
  assert.equal(sent.at(-1).kind, 'nativeFigureClose');
  assert.equal(window.islandNativeFigurePresenting, undefined);
  const previous = changed;
  present(id, true);
  assert.equal(changed, previous);
  window.dispatchEvent(new Event('resize'));
  assert.equal(frames.size, 0);
});
