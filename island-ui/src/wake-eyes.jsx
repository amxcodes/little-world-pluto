import React from 'react';

// Audio changes transforms only; the listening surface keeps a stable footprint.
export function WakeEyes({ levels = [], phase = 'thinking' }) {
  const level = values => Math.min(1, Math.sqrt(values.reduce((sum, value) => sum + Math.max(0, Math.min(1, Number(value) || 0)), 0) / Math.max(1, values.length)));
  const middle = Math.ceil(levels.length / 2);
  return <div className="wake-audio-eyes" data-phase={phase} aria-hidden="true">
    {[levels.slice(0, middle), levels.slice(middle)].map((band, index) => <i key={index} style={{ '--eye-level': phase === 'listening' ? level(band) : 0 }}/>) }
  </div>;
}
