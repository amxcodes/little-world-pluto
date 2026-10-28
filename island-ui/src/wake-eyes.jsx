import React from 'react';

// One envelope drives both eyes, so every frame stays symmetrical.
export function WakeEyes({ levels = [], phase = 'thinking' }) {
  let total = 0;
  for (const sample of levels) total += Math.max(0, Math.min(1, Number(sample) || 0));
  const energy = phase === 'listening' ? Math.sqrt(total / Math.max(1, levels.length)) : 0;
  return <div className="wake-audio-eyes" data-phase={phase} style={{ '--voice-energy': energy, '--eye-level': energy }} aria-hidden="true"><i /><i /></div>;
}
