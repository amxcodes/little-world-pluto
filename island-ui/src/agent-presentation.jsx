import React, { useState } from 'react';
import { AssemblyVisual, SystemMap, LineChart } from './agent-visuals.jsx';
import { WhiteboardSketch } from './agent-whiteboard.jsx';

const tones = new Set(['primary', 'neutral', 'info', 'success', 'warning', 'danger']);
const toneName = (value, fallback = 'neutral') => tones.has(value) ? value : fallback;

function ElementView({ element }) {
  if (!element || typeof element !== 'object') return null;
  switch (element.type) {
    case 'heading': {
      const Heading = ({ 1: 'h2', 2: 'h3', 3: 'h4' })[element.level] || 'h3';
      return <Heading className="agent-ui-heading">{element.text}</Heading>;
    }
    case 'text': return <p className={'agent-ui-text tone-' + toneName(element.tone, 'primary')}>{element.text}</p>;
    case 'badge': return <span className={'agent-ui-badge tone-' + toneName(element.tone)}>{element.label}</span>;
    case 'stat': return <div className="agent-ui-stat"><span>{element.label}</span><strong>{element.value}</strong>{element.detail && <small>{element.detail}</small>}</div>;
    case 'key_value': return <dl className="agent-ui-key-values">{element.items.map((item, index) => <div key={index}><dt>{item.key}</dt><dd>{item.value}</dd></div>)}</dl>;
    case 'callout': return <aside className={'agent-ui-callout tone-' + toneName(element.tone)}>{element.title && <strong>{element.title}</strong>}<p>{element.text}</p></aside>;
    case 'list': {
      const List = element.ordered ? 'ol' : 'ul';
      return <List className="agent-ui-list">{element.items.map((item, index) => <li key={index}>{item}</li>)}</List>;
    }
    case 'checklist': return <ul className="agent-ui-checklist">{element.items.map((item, index) => <li key={index} className={'state-' + item.state} aria-label={`${item.state.replace('_', ' ')}: ${item.text}`}><span aria-hidden="true">{item.state === 'done' ? '✓' : item.state === 'in_progress' ? '•' : '○'}</span>{item.text}</li>)}</ul>;
    case 'table': return <div className="agent-ui-table-scroll" tabIndex={0} aria-label="Result table"><table className="agent-ui-table"><thead><tr>{element.columns.map((column, index) => <th key={index} scope="col">{column}</th>)}</tr></thead><tbody>{element.rows.map((row, rowIndex) => <tr key={rowIndex}>{row.map((cell, index) => <td key={index}>{cell}</td>)}</tr>)}</tbody></table></div>;
    case 'code': return <div className="agent-ui-code">{element.language && <span>{element.language}</span>}<pre><code>{element.code}</code></pre></div>;
    case 'quote': return <blockquote className="agent-ui-quote"><p>{element.text}</p>{element.attribution && <cite>{element.attribution}</cite>}</blockquote>;
    case 'progress': {
      const max = Number.isFinite(element.max) && element.max > 0 ? element.max : 100;
      const value = Math.max(0, Math.min(max, Number(element.value) || 0));
      return <div className="agent-ui-progress"><div><strong>{element.label}</strong><span>{Math.round(value / max * 100)}%</span></div><progress aria-label={element.label} value={value} max={max} />{element.detail && <small>{element.detail}</small>}</div>;
    }
    case 'timeline': return <ol className="agent-ui-timeline">{element.items.map((item, index) => <li key={index} className={'state-' + (item.state || 'todo')}><i role="img" aria-label={item.state || 'todo'} /><span><strong>{item.label}</strong>{item.detail && <small>{item.detail}</small>}</span></li>)}</ol>;
    case 'bar_chart': {
      const max = Math.max(1, ...element.items.map(item => Number(item.value) || 0));
      return <div className="agent-ui-chart"><strong>{element.label}</strong>{element.items.map((item, index) => <div className="agent-ui-chart-row" key={index}><span>{item.label}</span><div><i style={{ width: `${Math.max(0, Math.min(100, (item.value / max) * 100))}%` }} /></div><b>{item.value.toLocaleString()}</b></div>)}</div>;
    }
    case 'file_change': return <div className="agent-ui-file"><span className={'agent-ui-badge tone-' + (element.status === 'deleted' ? 'danger' : element.status === 'added' ? 'success' : 'neutral')}>{element.status}</span><code title={element.path}>{element.path}</code>{element.summary && <small>{element.summary}</small>}</div>;
    case 'metric_grid': return <div className="agent-ui-metric-grid">{element.items.map((item, index) => <div key={index}><small>{item.label}</small><strong>{item.value}</strong>{item.detail && <span>{item.detail}</span>}</div>)}</div>;
    case 'status_strip': return <div className="agent-ui-status-strip">{element.items.map((item, index) => <span key={index} className={'tone-' + toneName(item.tone)}><i />{item.label}</span>)}</div>;
    case 'phase_card': return <div className={'agent-ui-phase state-' + (element.state || 'todo')}><span className="agent-ui-phase-mark" aria-hidden="true" /><div><strong>{element.title}</strong>{element.detail && <small>{element.detail}</small>}</div></div>;
    case 'comparison': return <div className="agent-ui-comparison" aria-label="Comparison">{[element.left, element.right].map((side, index) => <div key={index} className={'tone-' + toneName(side.tone)}><span>{side.label}</span><p>{side.text}</p></div>)}</div>;
    case 'black_hole': return <BlackHoleVisual />;
    case 'space_station': return <AssemblyVisual station />;
    case 'human_scene': return <div className="agent-ui-text">{element.steps?.map((step, index) => <p key={index}><strong>{step.label}</strong> {step.detail}</p>)}</div>;
    case 'assembly': return <AssemblyVisual label={element.label} parts={element.parts} />;
    case 'system_map': return <SystemMap label={element.label} nodes={element.nodes} links={element.links} />;
    case 'line_chart': return <LineChart label={element.label} unit={element.unit} points={element.points} />;
    case 'waveform': return <WaveformVisual label={element.label} values={element.values} />;
    case 'heatmap': return <HeatmapVisual label={element.label} columns={element.columns} rows={element.rows} />;
    case 'cross_section': return <CrossSectionVisual label={element.label} layers={element.layers} />;
    case 'visual_scene': return <VisualScene label={element.label} frames={element.frames} />;
    case 'whiteboard_sketch': return <WhiteboardSketch {...element} />;
    case 'section': return <section className="agent-ui-section">{element.title && <h3>{element.title}</h3>}{element.elements.map((child, index) => <ElementView key={index} element={child} />)}</section>;
    case 'columns': return <div className="agent-ui-columns">{element.columns.map((column, index) => <div key={index}>{column.map((child, childIndex) => <ElementView key={childIndex} element={child} />)}</div>)}</div>;
    case 'divider': return <hr className="agent-ui-divider" />;
    default: return null;
  }
}

function BlackHoleVisual() {
  const [focus, setFocus] = useState(0);
  const moments = [
    ['Light', 'Gravity bends light from the disk around the dark centre.'],
    ['Shadow', 'The dark region is the observed shadow; the horizon itself is not directly visible.'],
    ['Disk', 'Hot matter forms a flattened disk outside the horizon.'],
  ];
  return <figure className={'agent-ui-visual agent-ui-black-hole focus-' + focus}>
    <svg viewBox="0 0 440 188" role="img" aria-label="Scientific illustration of a black-hole shadow, event horizon, gravitationally lensed light, and a flattened accretion disk. Not to scale.">
      <path className="agent-ui-lensed-light" pathLength="1" d="M42 103 C102 43 156 54 190 80 C204 91 211 92 220 92 C229 92 236 91 250 80 C284 54 338 43 398 103" />
      <path className="agent-ui-accretion-back" d="M42 108 C106 70 160 71 192 91 C205 99 211 101 220 101 C229 101 235 99 248 91 C280 71 334 70 398 108" />
      <ellipse className="agent-ui-photon-ring" cx="220" cy="101" rx="45" ry="28" />
      <ellipse className="agent-ui-shadow" cx="220" cy="101" rx="37" ry="22" />
      <ellipse className="agent-ui-horizon" cx="220" cy="101" rx="31" ry="18" />
      <path className="agent-ui-accretion-front" d="M42 111 C106 148 160 145 192 121 C205 111 211 109 220 109 C229 109 235 111 248 121 C280 145 334 148 398 111" />
      <path className="agent-ui-visual-leader" d="M99 53 H152 L181 77" /><text className="agent-ui-svg-label" x="42" y="48">Lensed light</text>
      <path className="agent-ui-visual-leader" d="M287 120 H323 L342 135" /><text className="agent-ui-svg-label" x="326" y="151">Shadow</text>
      <path className="agent-ui-visual-leader" d="M220 126 V155" /><text className="agent-ui-svg-label" x="220" y="174" textAnchor="middle">Event horizon</text>
    </svg>
    <div className="agent-ui-visual-steps" aria-label="Explore the black hole illustration">{moments.map(([name], index) => <button type="button" key={name} aria-pressed={focus === index} onClick={() => setFocus(index)}>{name}</button>)}</div>
    <figcaption><span>{moments[focus][1]}</span><span>Not to scale</span></figcaption>
  </figure>;
}

function VisualScene({ label, frames }) {
  const [active, setActive] = useState(0);
  const items = Array.isArray(frames) ? frames.slice(0, 4) : [];
  if (!items.length) return null;
  const selected = Math.min(active, items.length - 1);
  const frame = items[selected];
  // Keep provider-authored labels readable, even in older saved scenes.
  const labels = [];
  for (const item of frame.marks.filter(mark => mark.kind === 'label')) {
    const candidates = [[0,0],[0,-8],[0,8],[-8,0],[8,0],[0,-16],[0,16]].map(([dx,dy]) => ({ x: Math.max(4, Math.min(96,item.x+dx)), y: Math.max(4,Math.min(66,item.y+dy)) }));
    const point = candidates.find(point => labels.every(other => Math.hypot(point.x-other.x,point.y-other.y) >= 6)) || candidates[0];
    labels.push({ ...item, ...point, originalX: item.x, originalY: item.y });
  }
  const mark = (item, index) => {
    const tone = 'tone-' + toneName(item.tone);
    const className = `agent-ui-scene-mark ${tone}`;
    switch (item.kind) {
      case 'dot': return <circle key={index} className={className + ' is-dot'} cx={item.x} cy={item.y} r={item.r} />;
      case 'ring': return <circle key={index} className={className} cx={item.x} cy={item.y} r={item.r} />;
      case 'ellipse': return <ellipse key={index} className={className} cx={item.x} cy={item.y} rx={item.rx} ry={item.ry} />;
      case 'line': return <line key={index} className={className} x1={item.x1} y1={item.y1} x2={item.x2} y2={item.y2} />;
      case 'curve': return <path key={index} className={className} d={`M ${item.x1} ${item.y1} Q ${item.cx} ${item.cy} ${item.x2} ${item.y2}`} />;
      case 'label': return null;
      default: return null;
    }
  };
  return <figure className="agent-ui-visual agent-ui-scene">
    <figcaption>{label}</figcaption>
    <svg key={selected} viewBox="0 0 100 70" preserveAspectRatio="xMidYMid meet" role="img" aria-label={`${label}: ${frame.label}${frame.caption ? '. ' + frame.caption : ''}`}>
      {frame.marks.map(mark)}
      {labels.map((item,index) => <g key={index} className="agent-scene-pin"><line x1={item.originalX} y1={item.originalY} x2={item.x} y2={item.y} /><circle cx={item.x} cy={item.y} r="2.4" /><text x={item.x} y={item.y+1} textAnchor="middle">{index+1}</text></g>)}
    </svg>
    {!!labels.length && <ol className="agent-scene-legend">{labels.map((item,index) => <li key={index}><span>{index+1}</span>{item.text}</li>)}</ol>}
    {items.length > 1 && <div className="agent-ui-visual-steps" aria-label="Visual explanation stages">{items.map((item, index) => <button type="button" key={index} aria-pressed={selected === index} onClick={() => setActive(index)}>{item.label}</button>)}</div>}
    {frame.caption && <p className="agent-ui-scene-caption">{frame.caption}</p>}
  </figure>;
}

function WaveformVisual({ label, values }) {
  const samples = (Array.isArray(values) ? values : []).slice(0, 64).map(value => Math.max(-1, Math.min(1, Number(value) || 0)));
  if (samples.length < 2) return null;
  const path = samples.map((value, index) => `${index ? 'L' : 'M'} ${(index / (samples.length - 1) * 300).toFixed(1)} ${(30 - value * 24).toFixed(1)}`).join(' ');
  return <figure className="agent-ui-visual agent-ui-waveform"><figcaption>{label}</figcaption><svg viewBox="0 0 300 60" preserveAspectRatio="none" role="img" aria-label={`${label} waveform`}><path className="agent-ui-wave-axis" d="M0 30 H300" /><path className="agent-ui-wave-path" pathLength="1" d={path} /></svg></figure>;
}

function HeatmapVisual({ label, columns, rows }) {
  const headings = (Array.isArray(columns) ? columns : []).slice(0, 8);
  const entries = (Array.isArray(rows) ? rows : []).slice(0, 8);
  if (headings.length < 2 || entries.length < 2) return null;
  return <figure className="agent-ui-visual agent-ui-heatmap"><figcaption>{label}</figcaption><div className="agent-ui-heat-grid" style={{ gridTemplateColumns: `minmax(48px,auto) repeat(${headings.length}, minmax(28px,1fr))` }}>
    <span />{headings.map((heading, index) => <span className="agent-ui-heat-heading" key={index}>{heading}</span>)}
    {entries.map((row, rowIndex) => <React.Fragment key={rowIndex}><span className="agent-ui-heat-heading">{row.label}</span>{headings.map((heading, columnIndex) => {
      const value = Math.max(0, Math.min(1, Number(row.values?.[columnIndex]) || 0));
      return <span className="agent-ui-heat-cell" key={`${rowIndex}:${columnIndex}`} title={`${row.label} · ${heading}: ${Math.round(value * 100)}%`} style={{ '--heat': value }}><i />{Math.round(value * 100)}</span>;
    })}</React.Fragment>)}
  </div></figure>;
}

function CrossSectionVisual({ label, layers }) {
  const entries = (Array.isArray(layers) ? layers : []).slice(0, 6);
  if (entries.length < 2) return null;
  return <figure className="agent-ui-visual agent-ui-cross-section"><figcaption>{label}</figcaption><div>{entries.map((layer, index) => <section key={index}><span>{layer.label}</span>{layer.detail && <small>{layer.detail}</small>}</section>)}</div></figure>;
}

export function AgentPresentationView({ presentation }) {
  if (!presentation || presentation.version !== 1 || !Array.isArray(presentation.elements)) return null;
  return <div className="agent-presentation">{presentation.title && <h2>{presentation.title}</h2>}{presentation.elements.map((element, index) => <ElementView key={index} element={element} />)}</div>;
}
