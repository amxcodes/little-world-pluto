import React, { useState } from 'react';
import { SplitRange } from './split-range.jsx';

const key = 'pluto.material.v1';
const defaults = { tint: 26, panels: 14, highlights: 34, softness: 10, saturation: 115, caustics: 24 };
const limits = { tint: [0, 100], panels: [0, 60], highlights: [0, 100], softness: [0, 20], saturation: [80, 160], caustics: [0, 70] };
const normalize = saved => Object.fromEntries(Object.entries(defaults).map(([name, fallback]) => [name,
  Number.isFinite(saved?.[name]) ? Math.min(limits[name][1], Math.max(limits[name][0], saved[name])) : fallback]));
export const glassStyle = material => material.enabled ? {
  '--glass-control-tint': material.tint / 100,
  '--glass-panel-opacity': material.panels / 100,
  '--glass-highlight-opacity': material.highlights / 100,
  '--glass-softness': `${material.softness}px`,
  '--glass-saturation': material.saturation / 100,
  '--glass-caustics': material.caustics / 100,
} : undefined;

export function useGlassTheme() {
  const [material, setMaterial] = useState(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(key) || '{}');
      return { enabled: saved?.enabled === true, motion: saved?.motion === true, ...normalize(saved) };
    } catch { return { enabled: false, motion: false, ...defaults }; }
  });
  const updateMaterial = change => setMaterial(previous => {
    const next = { ...previous, ...change, ...normalize({ ...previous, ...change }) };
    try { localStorage.setItem(key, JSON.stringify(next)); } catch {}
    return next;
  });
  return [material, updateMaterial];
}

export function GlassSettings({ material, updateMaterial, Toggle }) {
  return <div className="glass-preferences">
    <Toggle label="Liquid Glass" value={material.enabled} onChange={() => updateMaterial({ enabled: !material.enabled })}/>
    <Toggle label="Glass motion" disabled={!material.enabled} value={material.motion} onChange={() => updateMaterial({ motion: !material.motion })}/>
    {material.enabled && <fieldset className="glass-adjustments"><legend>Glass appearance</legend>
      {Object.entries({ tint: 'Control tint', panels: 'Panel opacity', highlights: 'Rim brightness', softness: 'Frost blur', saturation: 'Color vibrancy', caustics: 'Edge light' }).map(([name, label]) => <label key={name}>
        <span>{label}<output>{material[name]}{name === 'softness' ? ' px' : '%'}</output></span>
        <SplitRange label={label.toUpperCase()} min={limits[name][0]} max={limits[name][1]} unit={name === 'softness' ? 'px' : '%'} value={material[name]} onChange={event => updateMaterial({ [name]: Number(event.target.value) })}/>
      </label>)}
      <button type="button" onClick={() => updateMaterial(defaults)}>Reset appearance</button>
    </fieldset>}
  </div>;
}
