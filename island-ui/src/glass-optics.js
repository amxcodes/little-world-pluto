// Adapt the SVG renderer from GlassSurface for Pluto's live DOM surfaces. Each
// filter gets a shape map sized to that control; no image/canvas backdrop or
// continuous rendering loop is involved.
const glassSurfaceSelector = [
  '.tab-indicator', '.bar .icon-button', '.settings-nav-item', '.settings-topbar > button',
  '.settings-card', '.home-primary', '.home-actions', '.home-strip', '.control-tile',
  '.battery-tile', '.agent-composer', '.opencode-composer', '.wx-current', '.wx-daily',
  '.clip-card', '.media-session-panel', '.convert-options', '.settings-disclosure',
  '.output-row', '.agent-session-card', '.agent-question-card', '.calendar-popover',
  '.device-menu', '.voice-answer-text', '.clipboard-search', '.wx-search', '.agent-search',
  '.command-form', '.home-launcher',
  '.agent-config-chip', '.agent-project button', '.agent-browser-back',
  '.voice-answer-surface header button', '.power-section button', '.wx-heading button'
].join(',');

let nextGlassFilterId = 0;
const svgNs = 'http://www.w3.org/2000/svg';
const asSvg = (name, attrs = {}) => {
  const node = document.createElementNS(svgNs, name);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  return node;
};

const radiusFor = (style, width, height) => {
  const raw = style.borderTopLeftRadius || '0px';
  const [x, y = x] = raw.split(/\s+/).map(value => {
    const number = parseFloat(value) || 0;
    return value.endsWith('%') ? number * Math.min(width, height) / 100 : number;
  });
  return [Math.min(width / 2, x), Math.min(height / 2, y)];
};

const mapSvg = (width, height, radiusX, radiusY) => {
  const safeWidth = Math.max(2, Math.ceil(width));
  const safeHeight = Math.max(2, Math.ceil(height));
  const edge = Math.min(safeWidth, safeHeight) * 0.035;
  const rx = Math.max(0, radiusX);
  const ry = Math.max(0, radiusY);
  const blur = Math.max(0.6, Math.min(11, Math.min(safeWidth, safeHeight) * 0.055));
  const source = `<svg xmlns="http://www.w3.org/2000/svg" width="${safeWidth}" height="${safeHeight}" viewBox="0 0 ${safeWidth} ${safeHeight}"><defs><linearGradient id="red" x1="100%" y1="0%" x2="0%" y2="0%"><stop offset="0%" stop-color="#000" stop-opacity="0"/><stop offset="100%" stop-color="red"/></linearGradient><linearGradient id="blue" x1="0%" y1="0%" x2="0%" y2="100%"><stop offset="0%" stop-color="#000" stop-opacity="0"/><stop offset="100%" stop-color="blue"/></linearGradient><filter id="soft" filterUnits="userSpaceOnUse" x="0" y="0" width="${safeWidth}" height="${safeHeight}" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="${blur.toFixed(2)}"/></filter></defs><rect width="100%" height="100%" fill="#000"/><rect width="100%" height="100%" rx="${rx}" ry="${ry}" fill="url(#red)"/><rect width="100%" height="100%" rx="${rx}" ry="${ry}" fill="url(#blue)" style="mix-blend-mode:difference"/><rect x="${edge}" y="${edge}" width="${Math.max(0, safeWidth - edge * 2)}" height="${Math.max(0, safeHeight - edge * 2)}" rx="${Math.max(0, rx - edge)}" ry="${Math.max(0, ry - edge)}" fill="hsl(0 0% 50% / .93)" filter="url(#soft)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(source)}`;
};

const createFilter = (defs, amount) => {
  const id = `pluto-glass-${++nextGlassFilterId}`;
  const filter = asSvg('filter', { id, x: '0%', y: '0%', width: '100%', height: '100%', 'color-interpolation-filters': 'sRGB' });
  const image = asSvg('feImage', { x: '0', y: '0', width: '100%', height: '100%', preserveAspectRatio: 'none', result: 'map' });
  filter.appendChild(image);
  const baseScale = -amount * 24;
  const channels = [
    ['1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0', baseScale],
    ['0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0', baseScale + amount * 2],
    ['0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0', baseScale + amount * 4]
  ];
  const results = [];
  for (const [index, [matrix, scale]] of channels.entries()) {
    const displaced = `displaced-${index}`;
    const channel = `channel-${index}`;
    filter.appendChild(asSvg('feDisplacementMap', {
      in: 'SourceGraphic', in2: 'map', scale: scale.toFixed(2),
      xChannelSelector: 'R', yChannelSelector: 'G', result: displaced
    }));
    filter.appendChild(asSvg('feColorMatrix', { in: displaced, type: 'matrix', values: matrix, result: channel }));
    results.push(channel);
  }
  filter.appendChild(asSvg('feBlend', { in: results[0], in2: results[1], mode: 'screen', result: 'red-green' }));
  filter.appendChild(asSvg('feBlend', { in: 'red-green', in2: results[2], mode: 'screen', result: 'refracted' }));
  filter.appendChild(asSvg('feGaussianBlur', { in: 'refracted', stdDeviation: '0.35' }));
  defs.appendChild(filter);
  return { filter, image, id };
};

export function installGlassOptics(root, defs, refraction) {
  if (refraction <= 0) return () => {};
  const records = new Map();
  const resizing = new Set();
  let resizeTimer;
  const resizeObserver = new ResizeObserver(entries => {
    for (const { target } of entries) resizing.add(target);
    clearTimeout(resizeTimer);
    // Stretch the existing map during motion; rebuild its rounded edge map
    // once the layout settles instead of encoding SVG on every spring frame.
    resizeTimer = setTimeout(() => {
      for (const element of resizing) update(element);
      resizing.clear();
    }, 80);
  });

  const update = element => {
    const record = records.get(element);
    if (!record || !root.contains(element)) return;
    const width = element.offsetWidth;
    const height = element.offsetHeight;
    if (!width || !height) return;
    const style = getComputedStyle(element);
    const [radiusX, radiusY] = radiusFor(style, width, height);
    const key = `${width}x${height}:${radiusX.toFixed(1)}x${radiusY.toFixed(1)}`;
    if (key === record.key) return;
    record.key = key;
    record.image.setAttribute('href', mapSvg(width, height, radiusX, radiusY));
  };

  const register = element => {
    if (records.has(element)) return;
    const filter = createFilter(defs, Math.max(0, refraction));
    records.set(element, { ...filter, key: '' });
    element.style.setProperty('--glass-optics-filter', `url(#${filter.id})`);
    resizeObserver.observe(element);
    update(element);
  };
  for (const element of root.querySelectorAll(glassSurfaceSelector)) register(element);
  // Ignore streaming text and mutations made by the optics graph itself.
  // Inspect only added subtrees; collect detached surfaces once per frame.
  let frame = 0;
  const added = new Set();
  let prune = false;
  const flush = () => {
    frame = 0;
    if (prune) {
      for (const [element, record] of records) {
        if (root.contains(element)) continue;
        resizeObserver.unobserve(element);
        element.style.removeProperty('--glass-optics-filter');
        record.filter.remove();
        records.delete(element);
      }
      prune = false;
    }
    for (const node of added) {
      if (!root.contains(node)) continue;
      if (node.matches(glassSurfaceSelector)) register(node);
      for (const element of node.querySelectorAll(glassSurfaceSelector)) register(element);
    }
    added.clear();
  };
  const mutationObserver = new MutationObserver(mutations => {
    for (const mutation of mutations) {
      if (defs.contains(mutation.target)) continue;
      for (const node of mutation.addedNodes) {
        if (node.nodeType === 1 && !defs.contains(node)) added.add(node);
      }
      if ([...mutation.removedNodes].some(node => node.nodeType === 1)) prune = true;
    }
    if ((added.size || prune) && !frame) frame = requestAnimationFrame(flush);
  });
  mutationObserver.observe(root, { childList: true, subtree: true });
  return () => {
    mutationObserver.disconnect();
    cancelAnimationFrame(frame);
    clearTimeout(resizeTimer);
    resizing.clear();
    added.clear();
    resizeObserver.disconnect();
    for (const element of records.keys()) element.style.removeProperty('--glass-optics-filter');
    defs.replaceChildren();
  };
}
