import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

// Exercise the real capture callbacks without requiring a microphone or WebView.
const source = readFileSync(new URL('../src/main.jsx', import.meta.url), 'utf8');
const panel = source.slice(source.indexOf('function DictatePanel('), source.indexOf('  if (minimal) return <div className="wake-listening-surface"'))
  + '\nreturn { startDictation, stopDictation, cancelDictation }; }';
function harness(holdCapture = false, autoSubmit = false) {
  const effects = [], readers = [], recorders = [], sends = [], inserts = [], listeners = new Map();
  let resolveStream, opens = 0, stopped = 0;
  const track = { stop() { stopped++; }, addEventListener() {} };
  const stream = { getTracks: () => [track], getAudioTracks: () => [track] };
  class Recorder {
    static isTypeSupported() { return true; }
    constructor() { this.state = 'inactive'; recorders.push(this); }
    start() { this.state = 'recording'; }
    stop() { this.state = 'inactive'; this.onstop?.(); }
  }
  const context = {
    useRef: value => ({ current: value }), useState: value => [value, () => {}],
    useEffect: fn => effects.push(fn),
    navigator: { mediaDevices: { getUserMedia() { opens++; return new Promise(resolve => { resolveStream = resolve; }); } } },
    window: { MediaRecorder: Recorder, addEventListener: (key, fn) => listeners.set(key, fn), removeEventListener() {} },
    MediaRecorder: Recorder, Blob, Date, clearInterval, setInterval,
    FileReader: class { constructor() { readers.push(this); } readAsDataURL() {} },
    send: (...args) => sends.push(args),
    insert: text => inserts.push(text), holdCapture, autoSubmit,
  };
  vm.createContext(context);
  const api = vm.runInContext(panel + '\nDictatePanel({ speechSettings: { configured: true }, onClose() {}, onInsert: insert, holdCapture, autoSubmit })', context);
  const cleanups = effects.map(fn => fn()).filter(fn => typeof fn === 'function');
  return { api, recorders, readers, sends, inserts, listeners, resolve: () => resolveStream(stream), opens: () => opens, stopped: () => stopped, cleanup: () => cleanups.forEach(fn => fn()) };
}

test('repeated Record clicks open only one microphone request', async () => {
  const h = harness(); const first = h.api.startDictation();
  await h.api.startDictation(); assert.equal(h.opens(), 1);
  h.resolve(); await first; assert.equal(h.recorders.length, 1); h.cleanup();
});

test('capture readiness waits for the recorder start event', async () => {
  const h = harness(false, true);
  const pending = h.api.startDictation(); h.resolve(); await pending;
  assert.equal(h.sends.length, 0);
  h.recorders[0].onstart();
  assert.equal(h.sends[0][0], 'wakeCaptureReady');
  h.cleanup();
});

test('late recorder start event cannot revive a released hold', async () => {
  const h = harness(true, true);
  const pending = h.api.startDictation(); h.resolve(); await pending;
  h.listeners.get('island:hold-release')();
  h.recorders[0].onstart();
  assert.equal(h.sends.length, 0);
  h.cleanup();
});
test('unmount during microphone permission releases late stream', async () => {
  const h = harness(); const pending = h.api.startDictation();
  h.cleanup(); h.resolve(); await pending;
  assert.equal(h.stopped(), 1); assert.equal(h.recorders.length, 0);
});
test('cancel during microphone permission prevents invisible recording', async () => {
  const h = harness(); const pending = h.api.startDictation();
  h.api.cancelDictation(); h.resolve(); await pending;
  assert.equal(h.stopped(), 1); assert.equal(h.recorders.length, 0); h.cleanup();
});
test('cancel while encoding prevents transcription upload', async () => {
  const h = harness(); const pending = h.api.startDictation(); h.resolve(); await pending;
  h.recorders[0].ondataavailable({ data: new Blob(['voice']) });
  h.api.stopDictation(); assert.equal(h.readers.length, 1);
  h.api.cancelDictation(); h.readers[0].result = 'data:audio/webm;base64,dm9pY2U=';
  h.readers[0].onload(); assert.equal(h.sends.length, 0); h.cleanup();
});
test('cancel ignores a transcription response already in flight', async () => {
  const h = harness(); const pending = h.api.startDictation(); h.resolve(); await pending;
  h.recorders[0].ondataavailable({ data: new Blob(['voice']) }); h.api.stopDictation();
  h.readers[0].result = 'data:audio/webm;base64,dm9pY2U='; h.readers[0].onload();
  const requestId = h.sends[0][1].requestId;
  h.api.cancelDictation();
  h.listeners.get('island:speech-result')({ detail: { requestId, text: 'Do not send this' } });
  assert.equal(h.inserts.length, 0); h.cleanup();
});

test('releasing hold before microphone permission resolves stops the late recorder', async () => {
  const h = harness(true); const pending = h.api.startDictation();
  h.listeners.get('island:hold-release')(); h.resolve(); await pending;
  assert.equal(h.recorders[0].state, 'inactive'); assert.equal(h.stopped(), 1); h.cleanup();
});

test('releasing Enter ends keyboard hold capture', async () => {
  const h = harness(true); const pending = h.api.startDictation();
  h.resolve(); await pending;
  h.listeners.get('keyup')({ code: 'Enter' });
  assert.equal(h.recorders[0].state, 'inactive'); assert.equal(h.stopped(), 1); h.cleanup();
});

test('native Space-release fallback stops capture and submits all recorded audio', async () => {
  const h = harness(true); const pending = h.api.startDictation(); h.resolve(); await pending;
  const recorder = h.recorders[0];
  recorder.ondataavailable({ data: new Blob(['first']) });
  recorder.ondataavailable({ data: new Blob(['last']) });
  h.listeners.get('island:hold-release')();
  assert.equal(recorder.state, 'inactive');
  assert.equal(h.readers.length, 1);
  h.readers[0].result = 'data:audio/webm;base64,Zmlyc3RsYXN0';
  h.readers[0].onload();
  assert.equal(h.sends.filter(item => item[0] === 'speechTranscribe').length, 1);
  h.cleanup();
});
