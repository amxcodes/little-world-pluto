import React, { useId, useMemo, useState } from 'react';
import { resolveSketchLayout, routeSketchLinks } from './agent-whiteboard-layout.js';

function sketchPaint(value) {
  if (/^#[0-9a-f]{6}$/i.test(value || '')) return value;
  return ({ none: 'none', surface: 'var(--card)', neutral: '#26313a', primary: 'var(--accent)', info: '#175b78', success: '#216149', warning: '#795819', danger: '#7b3540' })[value];
}
function labelInk(fill) {
  if (fill === 'primary') return 'var(--accent-ink)';
  if (/^#[0-9a-f]{6}$/i.test(fill || '')) {
    const rgb = [1, 3, 5].map(i => parseInt(fill.slice(i, i + 2), 16) / 255).map(c => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
    return rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722 > .179 ? '#101114' : '#ffffff';
  }
  return ['neutral', 'info', 'success', 'warning', 'danger'].includes(fill) ? '#ffffff' : undefined;
}

// Models supply a few pen points. The app owns smoothing, bounds, ink and layout.
function inkPath({ points, closed = false, smooth = false, maxX = 100 }) {
  if (!Array.isArray(points) || points.length < 2) return '';
  const first = points[0];
  let path = `M ${first[0]} ${first[1]}`;
  // Straight contours retain corners. Smooth contours interpolate THROUGH every
  // supplied point, with clamped handles so short strokes cannot overshoot.
  for (let index = 0; index < points.length - 1 + Number(closed); index++) {
    const a = points[index % points.length], b = points[(index + 1) % points.length];
    if (!smooth) { path += ` L ${b[0]} ${b[1]}`; continue; }
    const prev = points[index - 1] || (closed ? points.at(-1) : a);
    const next = points[(index + 2) % points.length] || b;
    const after = !closed && index + 2 >= points.length ? b : next;
    const handle = (origin, direction, sign) => direction.map((v, axis) => {
      const span = Math.abs(b[axis] - a[axis]) / 3;
      return Math.max(0, Math.min(axis ? 70 : maxX, origin[axis] + sign * Math.max(-span, Math.min(span, v / 6))));
    });
    const c = handle(a, [b[0] - prev[0], b[1] - prev[1]], 1);
    const d = handle(b, [after[0] - a[0], after[1] - a[1]], -1);
    path += ` C ${c} ${d} ${b}`;
  }
  return `${path}${closed ? ' Z' : ''}`;
}

function itemLabel(item, index, hasChildren) {
  const size = hasChildren ? 2.6 : 3.2;
  const capacity = Math.floor((item.width - 3) / (size * .58));
  if (capacity < 3) return [String(index + 1)];
  const words = item.label.split(/\s+/), lines = [''];
  for (const word of words) {
    const line = lines.at(-1), next = line ? `${line} ${word}` : word;
    if (next.length <= capacity) lines[lines.length - 1] = next;
    else if (lines.length === 1 && word.length <= capacity) lines.push(word);
    else return [String(index + 1)];
  }
  return lines;
}

function compactRoute(route) {
  if (!route || route.length > 4) return false;
  return route.slice(1).reduce((length, point, index) => length
    + Math.abs(point[0] - route[index][0]) + Math.abs(point[1] - route[index][1]), 0) <= 42;
}

function StructuredDrawing({ items, links, routes, paths, strokes, marker, selected, focusedLink, onSelect }) {
  const ordered = [...items].sort((a, b) => Number(!!a.parent) - Number(!!b.parent));
  const focusedIds = focusedLink == null ? [] : [links[focusedLink]?.from, links[focusedLink]?.to];
  return <g className="sketch-structured">
    {ordered.map(item => {
      return <g key={item.id} className={`sketch-item ${selected === item.id || focusedIds.includes(item.id) || item.emphasis ? 'is-selected' : ''}`} role="button" tabIndex={0}
        style={{ '--sketch-fill': sketchPaint(item.fill) }} aria-label={item.label} onClick={() => onSelect(item.id)} onKeyDown={event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onSelect(item.id); } }}>
      <title>{item.label}</title>
      {item.outline?.length >= 3 ? <path d={inkPath({ points: item.outline.map(([x, y]) => [item.x + item.width * x / 100, item.y + item.height * y / 100]), closed: true, smooth: !!item.smooth, maxX: 160 })} />
        : ['ellipse', 'circle'].includes(item.shape) ? <ellipse cx={item.x + item.width / 2} cy={item.y + item.height / 2} rx={item.width / 2} ry={item.height / 2} />
        : item.shape === 'diamond' ? <path d={`M${item.x + item.width / 2} ${item.y}l${item.width / 2} ${item.height / 2}-${item.width / 2} ${item.height / 2}-${item.width / 2}-${item.height / 2}Z`} />
          : <rect x={item.x} y={item.y} width={item.width} height={item.height} rx="1.3" />}
    </g>})}
    {paths.map((path, index) => <path key={`stroke-${index}`} className={`agent-sketch-stroke role-${strokes[index].role || 'contour'}`} d={path} style={{ fill: strokes[index].closed ? sketchPaint(strokes[index].fill) : undefined }} />)}
    {routes.map((route, index) => route && <path key={`link-${index}`} className={`sketch-connector ${selected && (links[index].from === selected || links[index].to === selected) ? 'is-selected' : ''}`} d={`M${route.map(point => point.join(',')).join(' L')}`} markerEnd={`url(#${marker})`} />)}
    {ordered.map(item => {
      const index = items.indexOf(item);
      const hasChildren = items.some(child => child.parent === item.id && (!child.place || child.place === 'inside'));
      const lines = itemLabel(item, index, hasChildren);
      return <text key={`label-${item.id}`} className="sketch-item-label" x={item.x + item.width / 2} y={hasChildren ? item.y + 5 : item.y + item.height / 2 - (lines.length - 1) * 1.8}
        dominantBaseline="central" textAnchor="middle" style={{ fontSize: hasChildren ? '2.6px' : '3.2px', fill: labelInk(item.fill) }}>
        {lines.map((line, lineIndex) => <tspan key={lineIndex} x={item.x + item.width / 2} dy={lineIndex ? 3.6 : 0}>{line}</tspan>)}
      </text>})}
  </g>;
}

function BaseDrawing({ kind }) {
  if (kind === 'airplane') return <g className="agent-sketch-base" aria-hidden="true">
    <path d="M9 36Q17 30 38 31L48 12H57L53 31 79 32 87 23H93L90 35 94 38 89 41 52 40 61 58H51L37 40 14 39Z" />
    <path d="M17 33H25L29 36H14 M38 32 46 35 53 31 M37 40 45 36 52 40 M79 33 85 36 90 35" />
  </g>;
  if (kind === 'airfoil') return <g className="agent-sketch-base" aria-hidden="true">
    <path d="M12 39 Q38 25 69 34 Q80 37 89 40 Q49 47 12 39Z" />
  </g>;
  if (kind === 'connector') return <g className="agent-sketch-base" aria-hidden="true">
    <path d="M12 27 H38 Q43 27 43 32 V42 Q43 47 38 47 H12Z M43 32 H53 M43 42 H53 M66 28 H88 V46 H66Z M66 32 H84 V42 H66Z" />
  </g>;
  return null;
}

export function WhiteboardSketch({ label, background, base = 'blank', strokes, items = [], links = [], caption }) {
  const layout = useMemo(() => resolveSketchLayout(items, links), [items, links]);
  // Free-floating strokes are meaningful on a blank board. Once the provider
  // supplies structured parts, their raw canvas coordinates cannot track the
  // app's responsive layout and often cut across labels and connections.
  const visibleStrokes = useMemo(() => layout.auto && items.length ? (strokes || []).filter(stroke => stroke.on) : (strokes || []), [layout.auto, items.length, strokes]);
  const routes = useMemo(() => routeSketchLinks(layout.items, links, layout.width, visibleStrokes), [layout, links, visibleStrokes]);
  const paths = useMemo(() => visibleStrokes.map(stroke => {
    const parent = layout.items.find(item => item.id === stroke.on);
    const points = (stroke.points || []).map(([x, y]) => parent
      ? [parent.x + parent.width * x / 100, parent.y + parent.height * y / 100]
      : [x * layout.width / 100, y]);
    return inkPath({ ...stroke, points, maxX: layout.width });
  }), [visibleStrokes, layout]);
  const [selected, setSelected] = useState(null);
  const [focusedLink, setFocusedLink] = useState(null);
  const marker = `sketch-${useId().replace(/:/g, '')}`;
  if (base === 'human') return <p className="agent-ui-text">{caption || label}</p>;
  const detail = layout.items.find(item => item.id === selected);
  const activeLink = focusedLink != null && links[focusedLink] ? focusedLink : null;
  const focusedRelation = activeLink == null ? null : links[activeLink];
  const shownRoutes = routes.map((route, index) => activeLink != null ? (index === activeLink ? route : null)
    : selected ? (links[index].from === selected || links[index].to === selected ? route : null)
      : compactRoute(route) ? route : null);
  const partButtons = items.filter((item, index) => item.detail || itemLabel(layout.items[index], index,
    items.some(child => child.parent === item.id && (!child.place || child.place === 'inside')))[0] === String(index + 1));
  return <figure className="agent-ui-visual agent-whiteboard">
    <figcaption>{label}</figcaption>
    <svg style={{ aspectRatio: `${layout.width} / 70`, background: sketchPaint(background), '--text': labelInk(background) }} viewBox={`0 0 ${layout.width} 70`} preserveAspectRatio="xMidYMid meet" role="img" aria-label={`${label}. ${caption || 'Conceptual whiteboard sketch.'}`}>
      <defs><marker id={marker} viewBox="0 0 6 6" refX="5" refY="3" markerWidth="4" markerHeight="4" orient="auto"><path d="M0 0 6 3 0 6" fill="none" stroke="var(--accent)" /></marker></defs>
      <g transform={layout.auto ? 'scale(1.6 1)' : undefined}><BaseDrawing kind={base} /></g>
      <StructuredDrawing items={layout.items} links={links} routes={shownRoutes} paths={paths} strokes={visibleStrokes} marker={marker} selected={selected} focusedLink={activeLink} onSelect={id => { setFocusedLink(null); setSelected(selected === id ? null : id); }} />
    </svg>
    {!!partButtons.length && <div className="agent-visual-part-list" aria-label="Drawing details">{partButtons.map(item => <button type="button" key={item.id} aria-pressed={selected === item.id} onClick={() => { setFocusedLink(null); setSelected(selected === item.id ? null : item.id); }}>{item.label}</button>)}</div>}
    {detail?.detail && <p className="agent-ui-scene-caption">{detail.detail}</p>}
    {!!links.length && <ul className="sketch-link-legend" aria-label="Connections">{links.map((link, index) => <li key={index}><button type="button" aria-pressed={focusedLink === index} title={`${items.find(item => item.id === link.from)?.label} → ${items.find(item => item.id === link.to)?.label}`} onClick={() => { setSelected(null); setFocusedLink(focusedLink === index ? null : index); }}>{link.label || `${items.find(item => item.id === link.from)?.label} → ${items.find(item => item.id === link.to)?.label}`}</button></li>)}</ul>}
    {focusedRelation && <p className="agent-ui-scene-caption">{items.find(item => item.id === focusedRelation.from)?.label} → {items.find(item => item.id === focusedRelation.to)?.label}{focusedRelation.label ? ` · ${focusedRelation.label}` : ''}</p>}
    {caption && <p className="agent-ui-scene-caption">{caption}</p>}
  </figure>;
}
