// One lazily opened audio device; no samples, downloads, or idle audio work.
const key = 'pluto.ui-sounds';
let context, idleTimer, lastClick = 0;
export function readUiSoundsEnabled() {
  try { return localStorage.getItem(key) !== 'false'; } catch { return true; }
}
export function setUiSoundsEnabled(value) {
  try { localStorage.setItem(key, String(value)); } catch {}
  if (!value && context) void context.suspend().catch(() => {});
}
export function installUiSounds() {
  let disposed = false;
  const click = async event => {
    const button = event.target?.closest?.('button, [role="switch"]');
    if (!event.isTrusted || !button || button.disabled || button.closest('[inert]') || !readUiSoundsEnabled()) return;
    if (document.querySelector('[data-capture-state="recording"], .wake-audio-eyes[data-phase="listening"]')) return;
    const now = performance.now();
    if (now - lastClick < 20) return;
    lastClick = now;
    try {
      context ??= new AudioContext({ latencyHint: 'interactive' });
      clearTimeout(idleTimer);
      if (context.state === 'suspended') await context.resume();
      if (disposed || !readUiSoundsEnabled()) {
        if (context?.state === 'running') idleTimer = setTimeout(() => { void context?.suspend().catch(() => {}); }, 8000);
        return;
      }
      const start = context.currentTime;
      const gain = context.createGain();
      const tone = context.createOscillator();
      tone.type = 'sine';
      tone.frequency.setValueAtTime(620, start);
      tone.frequency.exponentialRampToValueAtTime(420, start + .055);
      gain.gain.setValueAtTime(0, start);
      gain.gain.linearRampToValueAtTime(.032, start + .009);
      gain.gain.exponentialRampToValueAtTime(.0001, start + .085);
      tone.connect(gain).connect(context.destination);
      tone.onended = () => { tone.disconnect(); gain.disconnect(); };
      tone.start(start); tone.stop(start + .095);
      idleTimer = setTimeout(() => { if (context?.state === 'running') void context.suspend().catch(() => {}); }, 8000);
    } catch { /* Audio availability never blocks an action. */ }
  };
  document.addEventListener('click', click);
  return () => {
    disposed = true;
    document.removeEventListener('click', click);
    clearTimeout(idleTimer);
    if (context) void context.close().catch(() => {});
    context = undefined;
  };
}
