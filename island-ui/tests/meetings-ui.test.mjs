import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { build } from 'esbuild';

const output = new URL('../node_modules/.cache/meetings-ssr.mjs', import.meta.url);
await mkdir(new URL('../node_modules/.cache/', import.meta.url), { recursive: true });
await build({ entryPoints: [fileURLToPath(new URL('../src/meetings-ui.jsx', import.meta.url))], outfile: fileURLToPath(output), bundle: true, platform: 'node', format: 'esm', packages: 'external' });
const { MeetingsWorkspace, wrapMeetingCaption, acquireStreams, micGateRms, removeCorrelatedPlayback, meetingSegmentWindow } = await import(output.href);
const render = meetings => renderToStaticMarkup(React.createElement(MeetingsWorkspace, { state: { meetings }, send() {}, onSettings() {}, Icon: () => null }));

assert.deepEqual(wrapMeetingCaption('One two three four five', 13, value => value.length), ['One two three', 'four five']);
assert.deepEqual(wrapMeetingCaption('  A   B  ', 3, value => value.length), ['A B']);
assert.ok(micGateRms(20) > micGateRms(55) && micGateRms(55) > micGateRms(100));

// The system reference should remove its delayed speaker copy while retaining
// independent near-end speech. A mic-only signal must pass unchanged.
const reference = new Float32Array(8192);
const write = 6400, delay = 721;
let seed = 23;
for (let index = 0; index < write; index++) {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  reference[index] = ((seed / 0xffffffff) * 2 - 1) * 0.18;
}
const packet = new Int16Array(1600);
const near = new Int16Array(1600);
for (let index = 0; index < packet.length; index++) {
  near[index] = Math.round(0.035 * Math.sin(index * 0.063) * 32768);
  packet[index] = Math.round((near[index] / 32768 + reference[write - packet.length - delay + index] * 0.6) * 32768);
}
const original = packet.slice();
assert.ok(removeCorrelatedPlayback(packet, reference, write, write) > 0.3);
const error = packet.reduce((sum, sample, index) => sum + Math.abs(sample - near[index]), 0) / packet.length;
assert.ok(error < 200, `speaker reference left too much echo: ${error}`);
const cleanMic = near.slice();
assert.equal(removeCorrelatedPlayback(cleanMic, reference, write, write), 0);
assert.deepEqual(cleanMic, near);

const idle = render({ engineInstalled: true, modelInstalled: true, sessions: [], source: 'microphone', notice: 'The local transcription connection ended unexpectedly. Finalized transcript text was saved.' });
assert.match(idle, /Record/);
assert.match(idle, /Library/);
assert.match(idle, /Start recording/);
assert.match(idle, /Retry/);
assert.match(idle, /data-layout-height="290"/);
assert.doesNotMatch(idle, /meeting-history panel/);

const live = render({ active: true, activeSessionId: 's1', engineInstalled: true, modelInstalled: true, source: 'both', partials: { system: 'A current spoken line', microphone: 'Another local line' }, sessions: [{ id: 's1', title: 'Planning', startedAt: 1, source: 'both', language: 'en-US', segments: [] }] });
assert.match(live, /System audio level/);
assert.match(live, /Mic audio level/);
assert.match(live, /A.*current.*spoken.*line/);
assert.match(live, /Another.*local.*line/);
assert.match(live, /meeting-caption-source/);
assert.match(live, /Pause/);
assert.match(live, /Stop/);
assert.doesNotMatch(live, /meeting-transcript-line/);
assert.match(render({ active: true, starting: true, engineInstalled: true, modelInstalled: true, source: 'both', sessions: [] }), /Preparing local transcription/);

const longSegments = Array.from({ length: 500 }, (_, index) => ({ text: `Line ${index}` }));
assert.deepEqual(meetingSegmentWindow(longSegments, 160, false).lines.map(line => line.text).at(-1), 'Line 159');
assert.deepEqual(meetingSegmentWindow(longSegments, 160, true).first, 340);
assert.deepEqual(meetingSegmentWindow(longSegments, 160, true).lines.map(line => line.text).at(-1), 'Line 499');

const micTrack = { stop() { this.stopped = true; }, getCapabilities() { return { echoCancellation: [true, 'all'] }; }, async applyConstraints(value) { this.echoCancellation = value.echoCancellation.ideal; } };
const fakeStream = (audio, video = []) => ({ getAudioTracks: () => audio, getTracks: () => [...audio, ...video] });
const microphone = fakeStream([micTrack]);
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaDevices: { getDisplayMedia: () => { throw Error('display picker must not open'); }, getUserMedia: () => Promise.resolve(microphone) } } });
const captured = await acquireStreams('both');
assert.deepEqual(captured.channels.map(channel => channel.source), ['system', 'microphone']);
assert.equal(captured.channels[0].native, true);
assert.equal(captured.audioTracks.length, 1);
assert.equal(micTrack.echoCancellation, 'all');
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {} });
assert.deepEqual((await acquireStreams('system')).channels.map(channel => channel.source), ['system']);

console.log('Meetings source validation and separate live captions pass.');
