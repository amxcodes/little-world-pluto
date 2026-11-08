import React, { useState } from 'react';
import { SplitRange } from './split-range.jsx';
import { appearanceKey, hyperKey, resolveAppearance, normalizeHyper, hyperDefaults } from './appearance-state.js';

const key = 'pluto.material.v1';
const defaults = { tint: 26, panels: 14, highlights: 34, softness: 10, saturation: 115, caustics: 24 };
const limits = { tint: [0, 100], panels: [0, 60], highlights: [0, 100], softness: [0, 20], saturation: [80, 160], caustics: [0, 70] };
const normalize = saved => Object.fromEntries(Object.entries(defaults).map(([name, fallback]) => [name,
  Number.isFinite(saved?.[name]) ? Math.min(limits[name][1], Math.max(limits[name][0], saved[name])) : fallback]));
const readSaved = (name, fallback) => { try { return JSON.parse(localStorage.getItem(name) || 'null') ?? fallback; } catch { return fallback; } };
export const glassStyle = material => material.mode === 'hyper-glass' ? {
  '--hyper-depth': material.hyper.depth / 100,
  '--hyper-frost': `${material.hyper.frost}px`,
  '--hyper-light': material.hyper.light / 100,
} : material.enabled ? {
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
      const saved = readSaved(key, {});
      const mode = resolveAppearance(readSaved(appearanceKey, null), saved);
      return { enabled: mode === 'glass', mode, motion: saved?.motion === true, ...normalize(saved),
        hyper: normalizeHyper(readSaved(hyperKey, {})) };
    } catch { return { enabled: false, mode: 'standard', motion: false, ...defaults, hyper: normalizeHyper() }; }
  });
  const updateMaterial = change => setMaterial(previous => {
    const mode = change.mode ? resolveAppearance(change.mode, previous) : previous.mode;
    const next = { ...previous, ...change, mode, enabled: mode === 'glass', ...normalize({ ...previous, ...change }),
      hyper: normalizeHyper({ ...previous.hyper, ...change.hyper }) };
    try {
      if (change.mode) localStorage.setItem(appearanceKey, JSON.stringify(mode));
      if (change.hyper) localStorage.setItem(hyperKey, JSON.stringify(next.hyper));
      // Mode selection never overwrites the existing Liquid Glass preferences.
      if (!change.mode && !change.hyper) {
        const { mode: ignoredMode, hyper: ignoredHyper, ...legacy } = next;
        localStorage.setItem(key, JSON.stringify(legacy));
      }
    } catch {}
    return next;
  });
  return [material, updateMaterial];
}

export function GlassSettings({ material, updateMaterial, Toggle }) {
  return <div className="glass-preferences">
    <div className="appearance-mode-picker" role="group" aria-label="Appearance mode">{[['standard','Standard'],['glass','Liquid Glass'],['hyper-glass','Hyper Glass']].map(([mode,label]) => <button type="button" key={mode} aria-pressed={material.mode === mode} className={material.mode === mode ? 'selected' : ''} onClick={() => updateMaterial({mode})}>{label}</button>)}</div>
    {material.mode !== 'hyper-glass' && <Toggle label="Glass motion" disabled={!material.enabled} value={material.motion} onChange={() => updateMaterial({ motion: !material.motion })}/>}
    {material.mode === 'hyper-glass' && <>
      <Toggle label="Hyper Glass motion" value={material.hyper.motion} onChange={() => updateMaterial({hyper:{motion:!material.hyper.motion}})}/>
      <fieldset className="glass-adjustments hyper-adjustments"><legend>Hyper Glass appearance</legend>
        {[["depth","Optical depth",100,'%'],["frost","Frost softness",16,'px'],["light","Edge light",80,'%']].map(([name,label,max,unit]) => <label key={name}><span>{label}<output>{material.hyper[name]}{unit}</output></span><SplitRange label={label} min={0} max={max} unit={unit} value={material.hyper[name]} onChange={e=>updateMaterial({hyper:{[name]:Number(e.target.value)}})}/></label>)}
        <div className="settings-choice"><span>Optics</span><div className="settings-segments" role="group" aria-label="Hyper Glass quality">{[['auto','Adaptive'],['efficient','Efficient']].map(([quality,label])=><button key={quality} type="button" className={material.hyper.quality===quality?'selected':''} aria-pressed={material.hyper.quality===quality} onClick={()=>updateMaterial({hyper:{quality}})}>{label}</button>)}</div></div>
        <button type="button" onClick={()=>updateMaterial({hyper:hyperDefaults})}>Reset Hyper Glass</button>
      </fieldset>
    </>}
    {material.enabled && <fieldset className="glass-adjustments"><legend>Glass appearance</legend>
      {Object.entries({ tint: 'Control tint', panels: 'Panel opacity', highlights: 'Rim brightness', softness: 'Frost blur', saturation: 'Color vibrancy', caustics: 'Edge light' }).map(([name, label]) => <label key={name}>
        <span>{label}<output>{material[name]}{name === 'softness' ? ' px' : '%'}</output></span>
        <SplitRange label={label.toUpperCase()} min={limits[name][0]} max={limits[name][1]} unit={name === 'softness' ? 'px' : '%'} value={material[name]} onChange={event => updateMaterial({ [name]: Number(event.target.value) })}/>
      </label>)}
      <button type="button" onClick={() => updateMaterial(defaults)}>Reset appearance</button>
    </fieldset>}
  </div>;
}
