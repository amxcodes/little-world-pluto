import React, { useEffect, useMemo, useRef, useState } from 'react';
import { readAgentPreferences } from './agents-ui.jsx';

const SOURCE_OPTIONS = [
  ['microphone', 'Microphone'],
  ['system', 'All PC audio'],
  ['both', 'Mic + PC audio'],
];
const LANGUAGE_OPTIONS = [
  ['en-US', 'English'],
  ['hi-IN', 'Hindi'],
  ['auto', 'Auto'],
];
const languageLabel = value => ({ 'en-US': 'English', 'hi-IN': 'Hindi', auto: 'Auto detect' }[value] || 'English');

const elapsedLabel = value => {
  const seconds = Math.max(0, Math.floor(Number(value) || 0));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
};

const dateLabel = value => value
  ? new Date(value * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
  : 'New meeting';

export function wrapMeetingCaption(text, maxWidth, measure = value => value.length * 9) {
  const lines = [];
  let line = '';
  for (const word of text.trim().split(/\s+/).filter(Boolean)) {
    const next = line ? `${line} ${word}` : word;
    if (line && measure(next) > maxWidth) { lines.push(line); line = word; }
    else line = next;
  }
  if (line) lines.push(line);
  return lines;
}

const transcriptSourceLabel = source => source === 'system' ? 'System' : source === 'microphone' ? 'Mic' : 'Mixed';
const segmentText = (session, segment) => `${session?.source === 'both' ? '[' + transcriptSourceLabel(segment.source) + '] ' : ''}${segment.text}`;
const sessionText = session => (session?.segments || []).map(segment => segmentText(session, segment)).join('\n');
export function meetingSegmentWindow(segments, limit, live) {
  const first = live ? Math.max(0, segments.length - limit) : 0;
  return { first, lines: segments.slice(first, live ? undefined : limit) };
}
// Reduce linearly correlated speaker playback in the microphone without
// attenuating unrelated near-end speech. Both streams share one AudioContext
// clock; the lag search covers the physical speaker-to-microphone delay.
export function removeCorrelatedPlayback(packet, reference, write, available) {
  if (packet.length < 320 || available < packet.length) return 0;
  const mask = reference.length - 1;
  const maxLag = Math.min(4800, available - packet.length);
  let bestLag = 0;
  let bestScore = 0;
  for (let lag = 0; lag <= maxLag; lag++) {
    let dot = 0, micEnergy = 0, refEnergy = 0;
    const base = write - packet.length - lag;
    for (let i = 0; i < packet.length; i += 32) {
      const mic = packet[i] / 32768;
      const ref = reference[(base + i) & mask];
      dot += mic * ref;
      micEnergy += mic * mic;
      refEnergy += ref * ref;
    }
    if (micEnergy < 0.0004 || refEnergy < 0.0004) continue;
    const score = dot * dot / (micEnergy * refEnergy);
    if (score > bestScore) { bestScore = score; bestLag = lag; }
  }
  if (bestScore < 0.16) return 0;
  bestScore = 0;
  let bestDot = 0, bestRefEnergy = 0;
  for (let lag = Math.max(0, bestLag - 4); lag <= Math.min(maxLag, bestLag + 4); lag++) {
    let dot = 0, micEnergy = 0, refEnergy = 0;
    const base = write - packet.length - lag;
    for (let i = 0; i < packet.length; i += 4) {
      const mic = packet[i] / 32768;
      const ref = reference[(base + i) & mask];
      dot += mic * ref;
      micEnergy += mic * mic;
      refEnergy += ref * ref;
    }
    const score = refEnergy > 0 ? dot * dot / (micEnergy * refEnergy) : 0;
    if (score > bestScore) { bestScore = score; bestLag = lag; bestDot = dot; bestRefEnergy = refEnergy; }
  }
  // Require a clearly shared waveform. A weak match is normal during double
  // talk and must not erase the person's own microphone speech.
  if (bestScore < 0.30) return 0;
  const gain = Math.max(-4, Math.min(4, bestDot / bestRefEnergy));
  const base = write - packet.length - bestLag;
  for (let i = 0; i < packet.length; i++) {
    const clean = packet[i] / 32768 - gain * reference[(base + i) & mask];
    packet[i] = Math.max(-32768, Math.min(32767, Math.round(clean * 32768)));
  }
  return bestScore;
}
const PCM_WORKLET_SOURCE = `
const removeCorrelatedPlayback = ${removeCorrelatedPlayback.toString()};
class PlutoPcm16Processor extends AudioWorkletProcessor {
  constructor(options) { super(); this.buffer = new Int16Array(1600); this.offset = 0; this.gateRms = options.processorOptions?.gateRms || 0; this.echoReference = !!options.processorOptions?.echoReference; this.nativeReference = !!options.processorOptions?.nativeReference; this.reference = this.echoReference ? new Float32Array(8192) : null; this.referenceWrite = 0; this.port.onmessage = event => { if (event.data?.type === 'flush') { this.flush(); this.port.postMessage({ flushed: true }); } else if (event.data?.type === 'reference' && this.reference) { const pcm = new Int16Array(event.data.audio); for (const sample of pcm) this.reference[this.referenceWrite++ & 8191] = sample / 32768; } }; }
  emit(length) { const packet = this.buffer.slice(0, length); if (this.echoReference && length === 1600) removeCorrelatedPlayback(packet, this.reference, this.referenceWrite, Math.min(this.referenceWrite, this.reference.length)); if (this.gateRms && length) { let energy = 0; for (let i = 0; i < length; i++) energy += packet[i] * packet[i]; if (Math.sqrt(energy / length) / 32768 < this.gateRms) packet.fill(0); } this.port.postMessage({ audio: packet.buffer }, [packet.buffer]); this.buffer = new Int16Array(1600); this.offset = 0; }
  flush() { if (this.offset) this.emit(this.offset); }
  process(inputs, outputs) {
    const input = inputs[0]?.[0];
    const echo = this.echoReference && !this.nativeReference ? inputs[1] : null;
    const output = outputs[0]?.[0];
    if (output) output.fill(0);
    if (!input) return true;
    const step = sampleRate / 16000;
    const count = Math.floor(input.length / step);
    for (let i = 0; i < count; i++) {
      const from = Math.floor(i * step);
      const to = Math.min(input.length, Math.floor((i + 1) * step));
      let sum = 0;
      for (let j = from; j < Math.max(from + 1, to); j++) sum += input[j] || 0;
      const sample = Math.max(-1, Math.min(1, sum / Math.max(1, to - from)));
      if (this.echoReference && !this.nativeReference) { let refSum = 0; for (let j = from; j < Math.max(from + 1, to); j++) refSum += echo?.[1] ? ((echo[0]?.[j] || 0) + echo[1][j]) * .5 : echo?.[0]?.[j] || 0; this.reference[this.referenceWrite++ & 8191] = refSum / Math.max(1, to - from); }
      this.buffer[this.offset++] = sample < 0 ? sample * 32768 : sample * 32767;
      if (this.offset === this.buffer.length) this.emit(this.offset);
    }
    return true;
  }
}
registerProcessor('pluto-pcm16', PlutoPcm16Processor);
`;

function exportTranscript(session, format) {
  const segments = session?.segments || [];
  const title = (session?.title || 'Meeting transcript').replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-').trim() || 'Meeting transcript';
  const formatTime = seconds => {
    const totalMs = Math.max(0, Math.floor((Number(seconds) || 0) * 1000));
    const hours = Math.floor(totalMs / 3_600_000);
    const minutes = Math.floor((totalMs % 3_600_000) / 60_000);
    const wholeSeconds = Math.floor((totalMs % 60_000) / 1000);
    const millis = totalMs % 1000;
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(wholeSeconds).padStart(2, '0')}.${String(millis).padStart(3, '0')}`;
  };
  let content;
  let extension;
  if (format === 'vtt') {
    extension = 'vtt';
    content = `WEBVTT\n\n${segments.map((segment, index) => {
      const start = Math.max(0, Number(segment.elapsedSeconds) || 0);
      const next = Number(segments[index + 1]?.elapsedSeconds);
      const end = Math.max(start + 1, Number.isFinite(next) && next > start ? next : start + 3);
      return `${formatTime(start)} --> ${formatTime(end)}\n${segmentText(session, segment)}`;
    }).join('\n\n')}`;
  } else if (format === 'md') {
    extension = 'md';
    content = `# ${session?.title || 'Meeting transcript'}\n\n${new Date((session?.startedAt || 0) * 1000).toLocaleString()}\n\n${segments.map(segment => `- **${elapsedLabel(segment.elapsedSeconds)}** ${segmentText(session, segment)}`).join('\n')}`;
  } else {
    extension = 'txt';
    content = segments.map(segment => `[${elapsedLabel(segment.elapsedSeconds)}] ${segmentText(session, segment)}`).join('\n');
  }
  const url = URL.createObjectURL(new Blob([content], { type: format === 'vtt' ? 'text/vtt;charset=utf-8' : 'text/plain;charset=utf-8' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = `${title}.${extension}`;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function acquireStreams(source) {
  const wantsSystem = source === 'system' || source === 'both';
  const wantsMic = source === 'microphone' || source === 'both';
  // Windows endpoint loopback supplies system playback directly. It does not
  // create a display track or trigger Chromium's oversized source chooser.
  const microphonePromise = wantsMic
    ? navigator.mediaDevices?.getUserMedia?.({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: false }, video: false })
    : Promise.resolve(null);
  if (wantsMic && !microphonePromise) {
    throw new Error('This Windows WebView does not expose audio capture.');
  }
  return Promise.allSettled([microphonePromise]).then(async results => {
    const failed = results.find(result => result.status === 'rejected');
    if (failed) {
      results.filter(result => result.status === 'fulfilled').forEach(result => result.value?.getTracks?.().forEach(track => track.stop()));
      throw failed.reason;
    }
    const [microphone] = results.map(result => result.value);
    const streams = [microphone].filter(Boolean);
    const micTracks = microphone?.getAudioTracks() || [];
    if (wantsMic && !micTracks.length) {
      streams.forEach(stream => stream.getTracks().forEach(track => track.stop()));
      throw new Error('Could not capture microphone audio from this device.');
    }
    // Use the browser's render-reference echo cancellation when available.
    // Plain echoCancellation:true can leave system playback in the mic track.
    for (const track of micTracks) {
      const supported = track.getCapabilities?.().echoCancellation;
      if (Array.isArray(supported) && supported.includes('all')) {
        try { await track.applyConstraints({ echoCancellation: { ideal: 'all' } }); } catch {}
      }
    }
    const channels = [...(wantsSystem ? [{ track: null, native: true, source: 'system', label: 'System' }] : []), ...micTracks.slice(0, 1).map(track => ({ track, source: 'microphone', label: 'Mic' }))];
    return { streams, audioTracks: micTracks, channels, label: source === 'both' ? 'Microphone + system audio' : source === 'system' ? 'System audio' : 'Microphone' };
  });
}

function pcm16At16k(samples, inputRate) {
  const outputRate = 16000;
  const step = inputRate / outputRate;
  const length = Math.floor(samples.length / step);
  if (!length) return new Int16Array(0);
  const output = new Int16Array(length);
  for (let i = 0; i < length; i++) {
    const from = Math.floor(i * step);
    const to = Math.min(samples.length, Math.floor((i + 1) * step));
    let sum = 0;
    for (let j = from; j < Math.max(from + 1, to); j++) sum += samples[j] || 0;
    const sample = Math.max(-1, Math.min(1, sum / Math.max(1, to - from)));
    output[i] = sample < 0 ? sample * 32768 : sample * 32767;
  }
  return output;
}

export function micGateRms(sensitivity) {
  const normalized = (100 - Math.max(20, Math.min(100, Number(sensitivity) || 55))) / 80;
  return 0.003 + normalized * normalized * 0.027;
}

function flushWorklet(channel) {
  const port = channel?.processor?.port;
  if (!port) return Promise.resolve();
  return new Promise(resolve => {
    channel.flushResolver = resolve;
    try { port.postMessage({ type: 'flush' }); }
    catch { channel.flushResolver = null; resolve(); return; }
    window.setTimeout(() => { if (channel.flushResolver === resolve) { channel.flushResolver = null; resolve(); } }, 150);
  });
}

/** Persistent capture/transport owner. It remains mounted while the user
 * changes pages so the native meeting worker and transcript keep running. */
export function MeetingCaptureController({ meeting, send }) {
  const capture = useRef(null);
  const meetingRef = useRef(meeting);
  meetingRef.current = meeting;
  const sendRef = useRef(send);
  sendRef.current = send;

  const releaseCapture = async (commit = false) => {
    const current = capture.current;
    if (!current) return;
    current.stopping = true;
    if (current.channels.some(channel => channel.native)) sendRef.current('meetingSystemAudioStop');
    if (commit) {
      await Promise.all(current.channels.map(async channel => {
        if (channel.socket?.readyState !== WebSocket.OPEN) return;
        await flushWorklet(channel);
        channel.stopping = true;
        if (channel.track) channel.track.enabled = false;
        try {
          const pending = channel.audioPackets ? new Promise(resolve => {
            channel.finalizeResolve = resolve;
            channel.finalizeTimer = window.setTimeout(resolve, 3000);
          }) : null;
          channel.socket.send(JSON.stringify({ type: 'input_audio_buffer.commit' }));
          if (pending) await pending;
        } catch {}
      }));
    }
    capture.current = null;
    current.channels.forEach(channel => {
      window.clearTimeout(channel.partialTimer);
      window.clearTimeout(channel.finalizeTimer);
      if (channel.socket?.readyState < WebSocket.CLOSING) {
        try { channel.socket.close(1000, 'meeting stopped'); } catch {}
      }
      try { channel.processor?.disconnect?.(); channel.processor?.port?.close?.(); } catch {}
      try { channel.silent?.disconnect(); channel.node?.disconnect(); channel.analyser?.disconnect(); } catch {}
    });
    current.streams.forEach(stream => stream.getTracks().forEach(track => track.stop()));
    try { await current.context?.close(); } catch {}
  };

  useEffect(() => {
    const onLoopbackFailed = event => {
      if (!capture.current || capture.current.stopping) return;
      void releaseCapture(false);
      sendRef.current('meetingCaptureFailed', { message: event.detail || 'System audio capture stopped.' });
    };
    window.addEventListener('plutoMeetingLoopbackFailed', onLoopbackFailed);
    window.startPlutoMeetingCapture = async source => {
      await releaseCapture(false);
      try {
        if (source !== 'system' && !navigator.mediaDevices) throw new Error('Microphone capture is not available in this WebView.');
        const acquired = await acquireStreams(source);
        const context = acquired.channels.some(channel => !channel.native) ? new AudioContext() : null;
        await context?.resume();
        const channels = acquired.channels.map(channel => {
          if (channel.native) return { ...channel, levels: Array(21).fill(0), processor: null, socket: null, partial: '', partialTimer: 0, audioPackets: 0 };
          const node = context.createMediaStreamSource(new MediaStream([channel.track]));
          const analyser = context.createAnalyser();
          analyser.fftSize = 256;
          analyser.smoothingTimeConstant = .7;
          node.connect(analyser);
          return { ...channel, node, analyser, data: new Uint8Array(analyser.frequencyBinCount), gateRms: channel.source === 'microphone' ? micGateRms(meetingRef.current?.micSensitivity) : 0, processor: null, socket: null, partial: '', partialTimer: 0, audioPackets: 0 };
        });
        const session = { ...acquired, context, channels, paused: false, stopping: false, startedAt: Date.now() };
        capture.current = session;
        acquired.audioTracks.forEach(track => track.addEventListener('ended', () => {
          if (capture.current === session && !session.stopping) sendRef.current('meetingCaptureFailed', { message: 'Microphone access ended. Your finalized transcript has been saved.' });
        }, { once: true }));
        sendRef.current('meetingCaptureStatus', { status: `Audio ready · ${acquired.label}` });
        return true;
      } catch (error) {
        const message = error?.name === 'NotAllowedError'
          ? 'Permission denied. Allow microphone access, then try again.'
          : error?.name === 'InvalidStateError'
            ? 'Windows needs a fresh click to start recording. Press Start recording again.'
          : error?.message || 'Could not start meeting audio capture.';
        sendRef.current('meetingCaptureFailed', { message });
        await releaseCapture(false);
        return false;
      }
    };
    window.plutoMeetingLoopbackSamples = encoded => {
      const current = capture.current;
      const channel = current?.channels.find(item => item.native);
      if (!channel || current.paused || current.stopping) return;
      const raw = atob(encoded);
      const bytes = Uint8Array.from(raw, letter => letter.charCodeAt(0));
      const pcm = new Int16Array(bytes.buffer);
      let energy = 0;
      for (const sample of pcm) energy += sample * sample;
      const level = Math.min(1, Math.sqrt(energy / Math.max(1, pcm.length)) / 7000);
      channel.levels = Array.from({ length: 21 }, (_, index) => Math.min(1, level * (0.55 + 0.45 * Math.sin(index * 0.62) ** 2)));
      if (channel.socket?.readyState === WebSocket.OPEN) {
        channel.socket.send(bytes.buffer);
        channel.audioPackets++;
      }
      const mic = current.channels.find(item => item.source === 'microphone');
      if (mic?.processor?.port) mic.processor.port.postMessage({ type: 'reference', audio: bytes.buffer }, [bytes.buffer]);
    };
    window.stopPlutoMeetingCapture = () => releaseCapture(true);
    window.readPlutoMeetingLevels = () => {
      const current = capture.current;
      if (!current || current.paused) return [];
      return current.channels.map(channel => {
        if (channel.native) return { label: channel.label, levels: channel.levels };
        channel.analyser.getByteFrequencyData(channel.data);
        const levels = Array.from({ length: 21 }, (_, index) => {
          const from = Math.floor(Math.pow(index / 21, 1.55) * channel.data.length);
          const to = Math.max(from + 1, Math.floor(Math.pow((index + 1) / 21, 1.55) * channel.data.length));
          let sum = 0;
          for (let bin = from; bin < to; bin++) sum += channel.data[bin];
          return Math.min(1, sum / (to - from) / 90);
        });
        return { label: channel.label, levels };
      });
    };
    window.setPlutoMeetingPaused = async paused => {
      const current = capture.current;
      if (!current || current.stopping) return false;
      if (Boolean(current.paused) === Boolean(paused)) return true;
      if (paused) {
        await Promise.all(current.channels.map(channel => flushWorklet(channel)));
        current.channels.forEach(channel => {
          if (channel.socket?.readyState === WebSocket.OPEN) {
            try { channel.socket.send(JSON.stringify({ type: 'input_audio_buffer.commit' })); } catch {}
          }
          if (channel.track) channel.track.enabled = false;
        });
        current.paused = true;
        try { await current.context?.suspend(); } catch {}
        sendRef.current('meetingCaptureStatus', { status: 'Paused · audio capture suspended' });
      } else {
        current.channels.forEach(channel => { if (channel.track) channel.track.enabled = true; });
        current.paused = false;
        try { await current.context?.resume(); } catch {}
        sendRef.current('meetingCaptureStatus', { status: `Transcribing locally · ${current.label}` });
      }
      return true;
    };
    return () => {
      window.removeEventListener('plutoMeetingLoopbackFailed', onLoopbackFailed);
      delete window.startPlutoMeetingCapture;
      delete window.plutoMeetingLoopbackSamples;
      delete window.stopPlutoMeetingCapture;
      delete window.readPlutoMeetingLevels;
      delete window.setPlutoMeetingPaused;
      void releaseCapture(false);
    };
  }, []);

  useEffect(() => {
    if (meeting?.active && !meeting.starting) void window.setPlutoMeetingPaused?.(!!meeting.paused);
  }, [meeting?.active, meeting?.starting, meeting?.paused]);

  useEffect(() => {
    if (!meeting?.active && !meeting?.starting && capture.current && !capture.current.stopping) {
      void releaseCapture(false);
    }
  }, [meeting?.active, meeting?.starting, meeting?.notice]);

  useEffect(() => {
    const current = capture.current;
    if (!meeting?.active || !meeting?.serverUrl || !current) return undefined;
    let cancelled = false;
    const beginAudio = async channel => {
      if (cancelled || capture.current !== current || channel.processor) return;
      if (channel.native) { channel.processor = true; return; }
      let processor;
      if (current.context.audioWorklet && typeof AudioWorkletNode !== 'undefined') {
        if (!current.workletReady) {
          const moduleUrl = URL.createObjectURL(new Blob([PCM_WORKLET_SOURCE], { type: 'text/javascript' }));
          current.workletReady = current.context.audioWorklet.addModule(moduleUrl).finally(() => URL.revokeObjectURL(moduleUrl));
        }
        try { await current.workletReady; }
        catch { throw new Error('The low-latency audio processor could not be initialized.'); }
        if (cancelled || capture.current !== current || channel.socket?.readyState !== WebSocket.OPEN) return;
        const reference = channel.source === 'microphone' ? current.channels.find(item => item.source === 'system') : null;
        processor = new AudioWorkletNode(current.context, 'pluto-pcm16', { numberOfInputs: reference && !reference.native ? 2 : 1, numberOfOutputs: 1, outputChannelCount: [1], processorOptions: { gateRms: channel.gateRms, echoReference: !!reference, nativeReference: !!reference?.native } });
        if (reference?.analyser) reference.analyser.connect(processor, 0, 1);
        processor.port.onmessage = event => {
          if (event.data?.flushed) { channel.flushResolver?.(); channel.flushResolver = null; return; }
          if (event.data?.audio && channel.socket?.readyState === WebSocket.OPEN && !current.paused && !channel.stopping) {
            channel.socket.send(event.data.audio);
            channel.audioPackets++;
          }
        };
      } else {
        processor = current.context.createScriptProcessor(4096, 1, 1);
        processor.onaudioprocess = event => {
          if (channel.socket?.readyState === WebSocket.OPEN && !current.paused && !channel.stopping) {
            const pcm = pcm16At16k(event.inputBuffer.getChannelData(0), current.context.sampleRate);
            if (pcm.length) {
              if (channel.gateRms) { let energy = 0; for (const sample of pcm) energy += sample * sample; if (Math.sqrt(energy / pcm.length) / 32768 < channel.gateRms) pcm.fill(0); }
              channel.socket.send(pcm.buffer); channel.audioPackets++;
            }
          }
          event.outputBuffer.getChannelData(0).fill(0);
        };
      }
      const silent = current.context.createGain();
      silent.gain.value = 0;
      channel.analyser.connect(processor);
      processor.connect(silent);
      silent.connect(current.context.destination);
      channel.processor = processor;
      channel.silent = silent;
      sendRef.current('meetingCaptureStatus', { status: `Transcribing locally · ${current.label}` });
    };

    current.channels.forEach(channel => {
      const socket = new WebSocket(meeting.serverUrl);
      socket.binaryType = 'arraybuffer';
      channel.socket = socket;
      socket.onmessage = event => {
        if (capture.current !== current) return;
        let payload;
        try { payload = JSON.parse(event.data); } catch { return; }
        if (payload.type === 'session.created') {
          socket.send(JSON.stringify({ type: 'session.update', session: { sample_rate: 16000, language: meeting?.language || 'en-US', automatic_punctuation: true, endpointing_ms: 900 } }));
        } else if (payload.type === 'session.updated') {
          void beginAudio(channel).catch(error => sendRef.current('meetingCaptureFailed', { message: error?.message || 'Could not initialize audio capture.' }));
        } else if (payload.type === 'conversation.item.input_audio_transcription.delta') {
          channel.partial += payload.delta || '';
          if (!channel.partialTimer) channel.partialTimer = window.setTimeout(() => {
            channel.partialTimer = 0;
            if (capture.current === current) sendRef.current('meetingTranscript', { text: channel.partial, source: channel.source, final: false, elapsedSeconds: (Date.now() - current.startedAt) / 1000 });
          }, 180);
        } else if (payload.type === 'conversation.item.input_audio_transcription.completed') {
          const text = payload.transcript || payload.item?.content?.find?.(part => part.type === 'input_audio')?.transcript || channel.partial;
          channel.partial = '';
          window.clearTimeout(channel.partialTimer);
          channel.partialTimer = 0;
          sendRef.current('meetingTranscript', { text, source: channel.source, final: true, elapsedSeconds: (Date.now() - current.startedAt) / 1000 });
          channel.audioPackets = 0;
          channel.finalizeResolve?.();
        } else if (payload.type === 'error') {
          channel.finalizeResolve?.();
          if (!current.stopping) sendRef.current('meetingCaptureFailed', { message: payload.error?.message || 'The local transcription engine reported an error.' });
        }
      };
      socket.onerror = () => { if (!current.stopping && capture.current === current) sendRef.current('meetingCaptureFailed', { message: `Could not connect the ${channel.label.toLowerCase()} audio to Pluto’s local transcription worker.` }); };
      socket.onclose = event => {
        channel.finalizeResolve?.();
        if (!cancelled && !current.stopping && capture.current === current && event.code !== 1000) {
          sendRef.current('meetingCaptureFailed', { message: `The ${channel.label.toLowerCase()} transcription connection ended unexpectedly. Saved lines remain in Library.` });
        }
      };
    });
    return () => { cancelled = true; void releaseCapture(false); };
  }, [meeting?.active, meeting?.serverUrl, meeting?.activeSessionId]);

  return null;
}
function MeetingWaveform({ active, paused, source }) {
  const [lanes, setLanes] = useState([]);
  useEffect(() => {
    if (!active || paused) { setLanes([]); return; }
    const sample = () => {
      if (document.visibilityState === 'visible') setLanes(window.readPlutoMeetingLevels?.() || []);
    };
    sample();
    const timer = window.setInterval(sample, 90);
    return () => window.clearInterval(timer);
  }, [active, paused]);
  const labels = source === 'both' ? ['System', 'Mic'] : [source === 'system' ? 'System' : 'Mic'];
  return <div className={'meeting-waveform ' + (paused ? 'is-paused' : '')} aria-label={paused ? 'Audio paused' : 'Live audio levels'}>
    {labels.map((label, laneIndex) => {
      const levels = lanes.find(lane => lane.label === label)?.levels || [];
      const average = levels.length ? levels.reduce((sum, level) => sum + level, 0) / levels.length : 0;
      return <div className={'meeting-wave-lane lane-' + laneIndex} key={label} role="meter" aria-label={`${label} audio level`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(average * 100)}>
        <span className="meeting-wave-label">{label}</span><span className="meeting-wave-bars" aria-hidden="true">{Array.from({ length: 21 }, (_, index) => <i key={index} style={{ '--bar-height': `${Math.max(3, Math.round((levels[index] || 0) * 36))}px` }} />)}</span>
      </div>;
    })}
  </div>;
}

export function MeetingsWorkspace({ state, send, Icon, onSettings }) {
  const meeting = state.meetings || {};
  const sessions = meeting.sessions || [];
  const [view, setView] = useState('record');
  const [selectedId, setSelectedId] = useState(null);
  const [search, setSearch] = useState('');
  const [visibleLineCount, setVisibleLineCount] = useState(160);
  const [reviewLineCount, setReviewLineCount] = useState(60);
  const [aiConsent, setAiConsent] = useState(null);
  const [removeArmed, setRemoveArmed] = useState(null);
  const [startingCapture, setStartingCapture] = useState(false);
  const [stoppingCapture, setStoppingCapture] = useState(false);
  const [recentLines, setRecentLines] = useState({});
  const [captionMetrics, setCaptionMetrics] = useState({ width: 580, font: '600 20px Arial' });
  const captionRef = useRef(null);
  const captionMeasure = useMemo(() => typeof document === 'undefined' ? null : document.createElement('canvas').getContext('2d'), []);
  const transcriptBodyRef = useRef(null);
  const followTranscript = useRef(true);
  const selectedSession = sessions.find(session => session.id === selectedId) || sessions[0] || null;
  const selectedIsLive = meeting.activeSessionId === selectedSession?.id;
  const segmentCount = selectedSession?.segments?.length || 0;
  const { first: firstVisibleLine, lines: visibleLines } = meetingSegmentWindow(selectedSession?.segments || [], visibleLineCount, selectedIsLive);
  const liveSession = sessions.find(session => session.id === meeting.activeSessionId);
  const lastLiveSegment = liveSession?.segments?.at(-1);
  useEffect(() => {
    followTranscript.current = true;
  }, [meeting.activeSessionId]);
  useEffect(() => { setVisibleLineCount(160); setReviewLineCount(60); setAiConsent(null); }, [selectedSession?.id]);
  useEffect(() => {
    if (!meeting.active || !lastLiveSegment?.text) { setRecentLines({}); return; }
    const source = lastLiveSegment.source || (meeting.source === 'system' ? 'system' : 'microphone');
    setRecentLines(previous => ({ ...previous, [source]: lastLiveSegment.text }));
    const timer = window.setTimeout(() => setRecentLines(previous => ({ ...previous, [source]: '' })), 1600);
    return () => window.clearTimeout(timer);
  }, [meeting.active, meeting.activeSessionId, lastLiveSegment?.text, liveSession?.segments?.length]);
  useEffect(() => {
    const node = captionRef.current;
    if (!node) return;
    const measure = () => {
      const style = getComputedStyle(node);
      const width = Math.max(120, node.clientWidth - 2);
      const font = style.font || `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      setCaptionMetrics(previous => previous.width === width && previous.font === font ? previous : { width, font });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [meeting.active, view]);
  useEffect(() => {
    const body = transcriptBodyRef.current;
    if (!body || meeting.activeSessionId !== selectedSession?.id || !followTranscript.current) return;
    body.scrollTop = body.scrollHeight;
  }, [meeting.activeSessionId, meeting.partials, selectedSession?.id, selectedSession?.segments?.length]);
  const visibleSessions = useMemo(() => {
    const term = search.trim().toLowerCase();
    if (!term) return sessions;
    return sessions.filter(session => session.title.toLowerCase().includes(term) || session.segments?.some(segment => segment.text.toLowerCase().includes(term)));
  }, [sessions, search]);
  const hasTranscript = segmentCount > 0;
  const optimization = meeting.optimization?.meetingId === selectedSession?.id ? meeting.optimization : null;
  const aiChanges = useMemo(() => optimization?.phase === 'review' ? (selectedSession?.segments || []).flatMap((segment, index) => optimization.proposed?.[index] && optimization.proposed[index] !== segment.text ? [{ index, original: segment.text, proposed: optimization.proposed[index], elapsedSeconds: segment.elapsedSeconds, source: segment.source }] : []) : [], [optimization, selectedSession?.segments]);
  const aiBusyElsewhere = meeting.optimization?.phase === 'running' && meeting.optimization.meetingId !== selectedSession?.id;
  const showAiStage = aiConsent?.id === selectedSession?.id || !!optimization?.phase;
  const aiProviderState = aiConsent ? state.agents?.[aiConsent.provider] : null;
  const aiModel = aiConsent ? Object.entries(readAgentPreferences().provider?.[aiConsent.provider]?.config || {}).find(([id]) => /model/i.test(id))?.[1] : '';
  const chooseAiProvider = provider => {
    if (!selectedSession) return;
    setAiConsent({ id: selectedSession.id, provider });
    send('meetingOptimizePrepare', { provider });
  };
  const toggleAiConsent = () => {
    if (!selectedSession) return;
    if (optimization?.phase === 'error') send('meetingOptimizeDecline', { id: selectedSession.id });
    if (aiConsent?.id === selectedSession.id) setAiConsent(null);
    else chooseAiProvider(readAgentPreferences().defaultProvider || 'opencode');
  };

  const start = async () => {
    setStartingCapture(true);
    const wantsSystem = meeting.source === 'system' || meeting.source === 'both';
    let started = false;
    try {
      if (wantsSystem) {
        // Native endpoint loopback does not move or cover the notch. The user
        // starts it explicitly here; closing the meeting stops it again.
        await new Promise((resolve, reject) => {
          const cleanup = () => {
            window.clearTimeout(timeout);
            window.removeEventListener('plutoMeetingLoopbackReady', onReady);
            window.removeEventListener('plutoMeetingLoopbackFailed', onFailed);
          };
          const onReady = () => { cleanup(); resolve(); };
          const onFailed = event => { cleanup(); reject(new Error(event.detail || 'Could not start system audio.')); };
          const timeout = window.setTimeout(() => {
            cleanup();
            reject(new Error('Windows did not start system-audio capture. Please retry.'));
          }, 4000);
          window.addEventListener('plutoMeetingLoopbackReady', onReady, { once: true });
          window.addEventListener('plutoMeetingLoopbackFailed', onFailed, { once: true });
          send('meetingSystemAudioStart');
        });
      }
      const ready = await window.startPlutoMeetingCapture?.(meeting.source || 'microphone');
      if (ready) { started = true; setView('record'); send('meetingStart'); }
    } catch (error) {
      send('meetingCaptureFailed', { message: error?.message || 'Could not begin local audio capture.' });
    } finally {
      if (wantsSystem && !started) send('meetingSystemAudioStop');
      setStartingCapture(false);
    }
  };

  const stop = async () => {
    if (stoppingCapture) return;
    setStoppingCapture(true);
    try { await window.stopPlutoMeetingCapture?.(); }
    finally { send('meetingStop'); setStoppingCapture(false); }
  };

  const togglePause = async () => {
    const nextPaused = !meeting.paused;
    const changed = await window.setPlutoMeetingPaused?.(nextPaused);
    if (changed) send(nextPaused ? 'meetingPause' : 'meetingResume');
  };

  const copyTranscript = async () => {
    if (!hasTranscript) return;
    try { await navigator.clipboard.writeText(sessionText(selectedSession)); } catch {}
  };

  const sourceLabel = source => source === 'both' ? 'Microphone + all PC audio' : source === 'system' ? 'All PC audio' : 'Microphone';
  const engineReady = meeting.engineInstalled && meeting.modelInstalled;

  const hasIssue = /audio|transcription|speech runtime|speech model|permission/i.test(meeting.notice || '') && /failed|could not|unexpected|denied|stopped|error|did not|ended/i.test(meeting.notice || '');
  const captionSources = meeting.source === 'both' ? ['system', 'microphone'] : [meeting.source === 'system' ? 'system' : 'microphone'];
  const captionLines = useMemo(() => captionSources.map(source => {
    const liveText = meeting.partials?.[source] || recentLines[source] || (meeting.source !== 'both' ? meeting.partial : '') || '';
    if (captionMeasure) captionMeasure.font = captionMetrics.font;
    const lines = wrapMeetingCaption(liveText, captionMetrics.width - (meeting.source === 'both' ? 57 : 0), value => captionMeasure?.measureText(value).width || value.length * 9);
    return { source, line: lines.at(-1) || '', lineIndex: lines.length - 1 };
  }), [meeting.partials, meeting.partial, meeting.source, recentLines, captionMetrics, captionMeasure]);

  return <section className="meetings-page" data-layout-height={view === 'library' ? (showAiStage ? 440 : 370) : meeting.active ? 310 : hasIssue ? 290 : 270} aria-label="Meetings">
    <header className="meetings-heading">
      <h1>Meetings</h1>
      <div className="meeting-view-tabs" role="tablist" aria-label="Meeting views" onKeyDown={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); const next = event.key === 'ArrowLeft' || event.key === 'Home' ? 'record' : 'library'; setView(next); event.currentTarget.querySelector('#meeting-' + next + '-tab')?.focus(); } }}>
        <button id="meeting-record-tab" type="button" role="tab" tabIndex={view === 'record' ? 0 : -1} aria-controls="meeting-record-panel" aria-selected={view === 'record'} className={view === 'record' ? 'is-selected' : ''} onClick={() => setView('record')}>Record</button>
        <button id="meeting-library-tab" type="button" role="tab" tabIndex={view === 'library' ? 0 : -1} aria-controls="meeting-library-panel" aria-selected={view === 'library'} className={view === 'library' ? 'is-selected' : ''} onClick={() => setView('library')}>Library{sessions.length > 0 && <span>{sessions.length}</span>}</button>
      </div>
    </header>

    {view === 'record' && <main id="meeting-record-panel" className="meeting-record-panel panel" role="tabpanel" aria-labelledby="meeting-record-tab">
      <div className="meeting-record-top">
        <span className={'meeting-record-state' + (meeting.active && !meeting.starting ? ' is-active' : '') + (meeting.paused ? ' is-paused' : '')}><i />{stoppingCapture ? 'Finishing transcript' : startingCapture ? 'Requesting audio' : meeting.starting ? 'Loading model' : meeting.paused ? 'Paused' : meeting.active ? 'Recording' : engineReady ? 'Ready' : 'Setup needed'}</span>
        <span className="meeting-record-language">{languageLabel(meeting.language || 'en-US')} · Local</span>
      </div>

      {meeting.active && meeting.starting ? <div className="meeting-preparing-stage" role="status"><span className="meeting-spinner" aria-hidden="true" /><strong>Preparing local transcription</strong><span>{meeting.phase || 'Loading the speech model…'}</span></div> : meeting.active ? <div className="meeting-live-stage">
        <MeetingWaveform active={meeting.active && !meeting.starting} paused={meeting.paused || stoppingCapture} source={meeting.source || 'microphone'} />
        <div className={'meeting-live-line' + (meeting.source === 'both' ? ' has-two-sources' : '')} ref={captionRef} aria-label="Current transcription lines">
          {captionLines.map(({ source, line, lineIndex }) => <div className="meeting-caption-lane" key={source}>
            {meeting.source === 'both' && <span className="meeting-caption-source">{source === 'system' ? 'System' : 'Mic'}</span>}
            {line ? <span className="meeting-caption-line" key={meeting.activeSessionId + '-' + source + '-' + lineIndex} aria-hidden="true">{line}</span> : <span className="meeting-listening">{stoppingCapture ? 'Finishing…' : meeting.paused ? 'Paused' : 'Listening…'}</span>}
          </div>)}
        </div>
        <span className="meeting-screen-reader-line" aria-live="polite">{lastLiveSegment?.text || ''}</span>
        <div className="meeting-record-controls">
          <span className="meeting-source-current">{sourceLabel(meeting.source)}</span>
          <button type="button" className="meeting-pill" disabled={meeting.starting || stoppingCapture} onClick={togglePause}><Icon name={meeting.paused ? 'Play' : 'Pause'} size={14} />{meeting.paused ? 'Resume' : 'Pause'}</button>
          <button type="button" className="meeting-pill is-stop" disabled={stoppingCapture} onClick={stop}><Icon name="Stop" size={14} />{stoppingCapture ? 'Finishing…' : 'Stop'}</button>
        </div>
      </div> : <div className="meeting-idle-stage">
        <div className="meeting-record-prompt"><strong>New meeting</strong></div>
        <div className="meeting-source-picker"><div role="group" aria-label="Audio source" className="meeting-source-options">{SOURCE_OPTIONS.map(([id, label]) => <button key={id} type="button" disabled={startingCapture} aria-pressed={(meeting.source || 'microphone') === id} className={(meeting.source || 'microphone') === id ? 'is-selected' : ''} onClick={() => send('meetingSourceSet', { source: id })}>{id === 'both' ? <span className="meeting-source-combo"><Icon name="Mic" size={13} /><Icon name="Speaker" size={13} /></span> : <Icon name={id === 'system' ? 'Speaker' : 'Mic'} size={14} />}{label}</button>)}</div></div>
        {engineReady ? <button type="button" className="meeting-start-button" disabled={startingCapture} onClick={start}>{startingCapture ? <span className="meeting-spinner" aria-hidden="true" /> : <Icon name="Record" size={15} />}{startingCapture ? 'Requesting audio…' : 'Start recording'}</button> : <button type="button" className="meeting-start-button" onClick={onSettings}>Set up recording</button>}
      </div>}

      {hasIssue && !meeting.active && <div className="meeting-inline-error" role="alert"><div><strong>Recording interrupted</strong><span>{meeting.notice}</span></div><div className="meeting-error-actions"><button type="button" className="meeting-pill" onClick={engineReady ? start : onSettings}><Icon name={engineReady ? 'Refresh' : 'Settings'} size={13} />{engineReady ? 'Retry' : 'Setup'}</button></div></div>}
      <p className="meeting-privacy-note">Transcript stays on this PC · Raw audio is not saved</p>
    </main>}

    {view === 'library' && (sessions.length ? <div id="meeting-library-panel" className="meetings-grid" role="tabpanel" aria-labelledby="meeting-library-tab">
      <aside className="meeting-history panel" aria-label="Saved meetings">
        <div className="meeting-history-top"><strong>Saved meetings</strong><span className="meeting-count">{sessions.length}</span></div>
        {sessions.length > 3 && <label className="meeting-search"><Icon name="Search" size={15} /><input value={search} onChange={event => setSearch(event.target.value)} placeholder="Find a meeting" aria-label="Search meetings" /></label>}
        <div className="meeting-session-list">{visibleSessions.map(session => <button key={session.id} type="button" className={'meeting-session ' + (selectedSession?.id === session.id ? 'is-selected' : '')} onClick={() => { setSelectedId(session.id); setVisibleLineCount(160); setRemoveArmed(null); }}>
          <span className="meeting-session-copy"><strong>{session.title || 'New meeting'}</strong><small>{dateLabel(session.startedAt)}</small></span>{meeting.activeSessionId === session.id && <i className="meeting-session-live" />}
        </button>)}{visibleSessions.length === 0 && <div className="meeting-history-empty">No matching meetings.</div>}</div>
      </aside>
      <main className="meeting-main panel">
        {selectedSession && <>
          <div className="meeting-transcript-header">
            <input key={selectedSession.id} className="meeting-title-input" aria-label="Meeting title" defaultValue={selectedSession.title || 'New meeting'} disabled={meeting.activeSessionId === selectedSession.id} onBlur={event => send('meetingSessionRename', { id: selectedSession.id, title: event.target.value })} />
            <div className="meeting-transcript-actions"><button type="button" disabled={!hasTranscript || selectedIsLive || aiBusyElsewhere || optimization?.phase === 'running' || optimization?.phase === 'review'} title={aiBusyElsewhere ? 'Finish the current AI review first' : 'Review this transcript with your configured ACP provider'} onClick={toggleAiConsent}><Icon name="Agents" size={13} />Optimize</button><button type="button" disabled={!hasTranscript} onClick={copyTranscript}><Icon name="Copy" size={13} />Copy</button><details className="meeting-export-menu"><summary><Icon name="Download" size={13} />Export</summary><div>{[['txt', 'Plain text'], ['md', 'Markdown'], ['vtt', 'WebVTT captions']].map(([format, label]) => <button key={format} type="button" disabled={!hasTranscript} onClick={event => { exportTranscript(selectedSession, format); event.currentTarget.closest('details').open = false; }}>{label}</button>)}</div></details><button type="button" className={removeArmed === selectedSession.id ? 'is-armed' : ''} disabled={meeting.activeSessionId === selectedSession.id || optimization?.phase === 'running'} onClick={() => { if (removeArmed === selectedSession.id) { send('meetingSessionDelete', { id: selectedSession.id }); setSelectedId(null); setRemoveArmed(null); } else setRemoveArmed(selectedSession.id); }}><Icon name="Trash" size={13} />{removeArmed === selectedSession.id ? 'Confirm' : 'Delete'}</button></div>
          </div>
          <div className="meeting-transcript-meta"><span>{dateLabel(selectedSession.startedAt)}</span><span>{sourceLabel(selectedSession.source)}</span><span>{languageLabel(selectedSession.language || 'en-US')}</span>{meeting.activeSessionId === selectedSession.id && <span className="meeting-capture-state">Live</span>}</div>
          {showAiStage ? <div className="meeting-ai-stage" key={selectedSession.id + ':ai'}>
            {aiConsent?.id === selectedSession.id && !optimization?.phase ? <div className="meeting-ai-consent">
              <strong>Optimize this transcript</strong>
              <p>Only this meeting’s {segmentCount} text lines will be sent to the selected ACP provider when you press Send. Its model may run remotely and its task history may retain the text. Raw audio and other meetings stay on this PC. You can review every proposed change before replacing anything.</p>
              <div className="meeting-ai-providers" role="group" aria-label="AI provider">{['opencode', 'codex'].map(provider => <button key={provider} type="button" className={aiConsent.provider === provider ? 'is-selected' : ''} aria-pressed={aiConsent.provider === provider} onClick={() => chooseAiProvider(provider)}>{provider === 'codex' ? 'Codex' : 'OpenCode'}{state.agents?.[provider]?.connected ? ' · ready' : ' · connecting'}</button>)}</div>
              <span className="meeting-ai-model">{aiModel ? `Model: ${aiModel}` : 'Using the provider’s selected model'}{!aiProviderState?.connected && (aiProviderState?.authRequired ? ' · Sign in from Agents' : ' · Connecting provider…')}</span>
              <div className="meeting-ai-actions"><button type="button" className="meeting-pill" onClick={() => setAiConsent(null)}>Cancel</button><button type="button" className="meeting-pill meeting-ai-send" disabled={!aiProviderState?.connected} onClick={() => { send('meetingOptimizeStart', { id: selectedSession.id, provider: aiConsent.provider }); setAiConsent(null); }}>Send transcript</button></div>
            </div>
              : optimization?.phase === 'running' ? <div className="meeting-ai-working" role="status"><span className="meeting-spinner" aria-hidden="true" /><strong>Reviewing with {optimization.provider === 'codex' ? 'Codex' : 'OpenCode'}</strong><span>The original transcript is unchanged.</span></div>
                : optimization?.phase === 'error' ? <div className="meeting-ai-working" role="alert"><strong>AI review could not finish</strong><span>{optimization.error || 'The provider did not return a valid transcript.'}</span><button type="button" className="meeting-pill" onClick={() => send('meetingOptimizeDecline', { id: selectedSession.id })}>Back to transcript</button></div>
                  : optimization?.phase === 'review' ? <div className="meeting-ai-review"><div className="meeting-ai-review-top"><strong>{aiChanges.length ? `${aiChanges.length} proposed changes` : 'No wording changes proposed'}</strong><span>Original transcript remains saved</span></div><div className="meeting-ai-diffs">{aiChanges.slice(0, reviewLineCount).map(change => <div className="meeting-ai-diff" key={change.index}><small>{elapsedLabel(change.elapsedSeconds)} · {transcriptSourceLabel(change.source)}</small><p><b>Original</b>{change.original}</p><p><b>Proposed</b>{change.proposed}</p></div>)}{aiChanges.length > reviewLineCount && <button type="button" className="meeting-more-lines" onClick={() => setReviewLineCount(count => count + 60)}>Show more changes</button>}</div><div className="meeting-ai-actions"><button type="button" className="meeting-pill" onClick={() => send('meetingOptimizeDecline', { id: selectedSession.id })}>Decline</button><button type="button" className="meeting-pill meeting-ai-send" onClick={() => send('meetingOptimizeAccept', { id: selectedSession.id })}>{aiChanges.length ? 'Accept changes' : 'Done'}</button></div></div> : null}
          </div> : <div className="meeting-transcript-body" key={selectedSession.id} ref={transcriptBodyRef} onScroll={event => { const node = event.currentTarget; followTranscript.current = node.scrollHeight - node.scrollTop - node.clientHeight < 48; }}>
            {selectedIsLive && firstVisibleLine > 0 && <button type="button" className="meeting-more-lines" onClick={() => { followTranscript.current = false; setVisibleLineCount(count => count + 160); }}>Show earlier lines</button>}
            {visibleLines.map((segment, index) => <p className="meeting-transcript-line" key={selectedSession.id + '-' + (firstVisibleLine + index)}><time>{elapsedLabel(segment.elapsedSeconds)}</time><span>{selectedSession.source === 'both' && <b className="meeting-transcript-source">{transcriptSourceLabel(segment.source)}</b>}{segment.text}</span></p>)}
            {meeting.activeSessionId === selectedSession.id && Object.entries(meeting.partials || {}).map(([source, text]) => text && <p className="meeting-transcript-line is-partial" key={source}><time>{elapsedLabel((Date.now() - selectedSession.startedAt * 1000) / 1000)}</time><span>{selectedSession.source === 'both' && <b className="meeting-transcript-source">{source === 'system' ? 'System' : 'Mic'}</b>}{text}</span></p>)}
            {!selectedSession.segments?.length && !Object.values(meeting.partials || {}).some(Boolean) && <div className="meeting-transcript-placeholder"><strong>No transcript yet</strong><span>{meeting.activeSessionId === selectedSession.id ? 'The first completed line will appear here.' : 'This meeting has no saved lines.'}</span></div>}
            {!selectedIsLive && segmentCount > visibleLineCount && <button type="button" className="meeting-more-lines" onClick={() => setVisibleLineCount(count => count + 160)}>Load more lines <span>{Math.min(visibleLineCount, segmentCount)} of {segmentCount}</span></button>}
          </div>}
        </>}
      </main>
    </div> : <div id="meeting-library-panel" className="meeting-library-empty panel" role="tabpanel" aria-labelledby="meeting-library-tab"><strong>No meetings saved</strong><span>Finished transcripts will appear here.</span><button type="button" className="meeting-pill" onClick={() => setView('record')}>Go to Record</button></div>)}
  </section>;
}
