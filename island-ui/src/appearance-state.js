export const appearanceKey = 'pluto.appearance.mode.v1';
export const hyperKey = 'pluto.hyper-glass.v1';
export const appearanceModes = ['standard', 'glass', 'hyper-glass'];
export const hyperDefaults = { motion: true, depth: 45, frost: 8, light: 48, quality: 'auto' };

export function normalizeHyper(value = {}) {
  return { motion: value.motion !== false,
    depth: Number.isFinite(value.depth) ? Math.max(0, Math.min(100, value.depth)) : hyperDefaults.depth,
    frost: Number.isFinite(value.frost) ? Math.max(0, Math.min(16, value.frost)) : hyperDefaults.frost,
    light: Number.isFinite(value.light) ? Math.max(0, Math.min(80, value.light)) : hyperDefaults.light,
    quality: value.quality === 'efficient' ? 'efficient' : 'auto' };
}
export function resolveAppearance(explicit, legacy) {
  return appearanceModes.includes(explicit) ? explicit : legacy?.enabled === true ? 'glass' : 'standard';
}
