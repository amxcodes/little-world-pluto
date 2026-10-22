import React, { useId, useState } from 'react';

// App-owned projection: providers never supply paths, CSS, scripts, or assets.
const project = ([x, y, z]) => [(x - y) * 34, (x + y) * 17 - z * 34];
const polygon = points => points.map(point => project(point).join(',')).join(' ');
const sizes = { block: [1.5, 1.2, .7], cylinder: [1, 1, 1.5], sphere: [1.2, 1.2, 1.2], panel: [2, 1.5, .15], solar_array: [1.4, 3, .15], truss: [3, .5, .5], module: [1, 1, 1.8], radiator: [1.4, 2.4, .15] };

function Solid({ part, gradient }) {
  const [x, y, z] = part.position;
  const [w, d, h] = part.size;
  const a = [x, y, z], b = [x + w, y, z], c = [x + w, y + d, z], e = [x, y + d, z];
  const top = point => [point[0], point[1], z + h];
  const at = (u, v, height = h) => project([x + w * u, y + d * v, z + height]);
  const segment = (p, q, key, className = 'iso-seam') => <line key={key} className={className} x1={p[0]} y1={p[1]} x2={q[0]} y2={q[1]} />;
  if (part.shape === 'sphere') {
    const [cx, cy] = at(.5, .5, h / 2);
    return <ellipse cx={cx} cy={cy} rx={Math.min(w, d) * 24} ry={h * 24} fill={`url(#${gradient}-sphere)`} className="iso-sphere" />;
  }
  if (part.shape === 'cylinder' || part.shape === 'module') {
    const [cx, cy] = at(.5, .5), [, bottom] = at(.5, .5, 0);
    const rx = Math.min(w, d) * 25, ry = Math.min(w, d) * 12;
    return <>
      <path className="iso-cylinder" fill={`url(#${gradient})`} d={`M ${cx-rx} ${cy} A ${rx} ${ry} 0 0 0 ${cx+rx} ${cy} V ${bottom} A ${rx} ${ry} 0 0 1 ${cx-rx} ${bottom} Z`} />
      <ellipse className="iso-top" cx={cx} cy={cy} rx={rx} ry={ry} />
      {part.shape === 'module' && <>{[.2, .8].map(f => <path key={f} className="iso-seam" d={`M ${cx-rx} ${cy+(bottom-cy)*f} a ${rx} ${ry} 0 0 0 ${rx*2} 0`} />)}<ellipse className="iso-port" cx={cx} cy={cy} rx={rx*.4} ry={ry*.4} /></>}
    </>;
  }
  return <>
    <polygon className="iso-left" points={polygon([e, c, top(c), top(e)])} />
    <polygon className="iso-right" points={polygon([b, c, top(c), top(b)])} />
    <polygon className={'iso-top ' + (part.shape === 'solar_array' ? 'iso-solar' : '')} points={polygon([top(a), top(b), top(c), top(e)])} />
    {['panel', 'solar_array', 'radiator'].includes(part.shape) && <>
      {Array.from({length: part.shape === 'solar_array' ? 8 : 5}, (_, i) => { const f = (i+1)/(part.shape === 'solar_array' ? 9 : 6); return segment(at(0, f), at(1, f), `v${i}`); })}
      {part.shape === 'solar_array' && [1/3, 2/3].map(f => segment(at(f, 0), at(f, 1), f))}
    </>}
    {part.shape === 'truss' && Array.from({length: 6}, (_, i) => <React.Fragment key={i}>{segment(at(i/6,1,0), at((i+1)/6,1), `a${i}`)}{segment(at(i/6,1), at((i+1)/6,1,0), `b${i}`)}</React.Fragment>)}
  </>;
}

// Based on NASA assembly descriptions. Deliberately grouped, not a current docking map.
const stationParts = [
  { label: 'Truss', detail: 'The external backbone supports power arrays, radiators, and equipment.', shape: 'truss', position: [-5, 0, 1.8], size: [10 / 1.25, .45, .4] },
  ...[-4.6, -2.8, 1.1, 2.8].map((x, i) => ({ label: `Solar wing ${i+1}`, detail: 'Photovoltaic arrays convert sunlight into electrical power. The wings here are grouped for clarity.', shape: 'solar_array', position: [x, -2.5, 2.2], size: [1.2, 4.8, .15] })),
  { label: 'Radiators', detail: 'Thermal-control panels reject waste heat into space.', shape: 'radiator', position: [-1.8, .6, 1.8], size: [1.4, 2.3, .15] },
  { label: 'Destiny', detail: 'The US laboratory supports scientific work aboard the station.', shape: 'module', position: [-.4, -.1, .2], size: [.9, .9, 1.5] },
  { label: 'Harmony & labs', detail: 'Harmony connects Destiny with the Columbus and Kibo laboratories. This group is simplified here.', shape: 'module', position: [.6, 1, .2], size: [.9, .9, 1.1] },
  { label: 'Unity', detail: 'A connecting node joining station elements.', shape: 'module', position: [-1.1, -.9, .2], size: [.9, .9, .8] },
  { label: 'Tranquility', detail: 'Hosts life-support and crew facilities, with access to the Cupola.', shape: 'module', position: [-2.2, -1, .2], size: [.9, .9, 1.1] },
  { label: 'Russian segment', detail: 'Zarya and Zvezda are part of the Russian segment. Individual modules and docking ports are grouped in this overview.', shape: 'module', position: [-1.1, -2.1, .2], size: [.9, .9, 1.4] },
];

export function AssemblyVisual({ label, parts, station = false }) {
  const [active, setActive] = useState(0);
  const gradient = useId().replace(/:/g, '');
  const source = (station ? stationParts : parts || []).slice(0, 12);
  const spacingX = Math.max(1, ...source.map(part => (part.size || sizes[part.shape] || sizes.block)[0])) + 1;
  const spacingY = Math.max(1, ...source.map(part => (part.size || sizes[part.shape] || sizes.block)[1])) + 1;
  const items = source.map((part, index) => ({ ...part, index, position: part.position || [(index % 3) * spacingX, Math.floor(index / 3) * spacingY, 0], size: part.size || sizes[part.shape] || sizes.block }));
  if (!items.length) return null;
  const selected = Math.min(active, items.length - 1);
  const bounds = items.flatMap(part => { const [x,y,z] = part.position, [w,d,h] = part.size; return [0,w].flatMap(dx => [0,d].flatMap(dy => [0,h].map(dz => project([x+dx,y+dy,z+dz])))); });
  const minX = Math.min(...bounds.map(p => p[0])) - 22, minY = Math.min(...bounds.map(p => p[1])) - 28;
  const width = Math.max(...bounds.map(p => p[0])) - minX + 22, height = Math.max(...bounds.map(p => p[1])) - minY + 28;
  const sorted = [...items].sort((a,b) => (a.position[0]+a.position[1]+a.position[2]) - (b.position[0]+b.position[1]+b.position[2]));
  return <figure className="agent-ui-visual agent-assembly">
    <figcaption>{station ? 'International Space Station · simplified isometric overview' : label}</figcaption>
    <svg className="agent-assembly-canvas" viewBox={`${minX} ${minY} ${width} ${height}`} role="img" aria-label={station ? 'Isometric station overview with solar wings, a lattice truss, radiator and cylindrical module groups. Use the part buttons to explore.' : `${label}. Select a part below for details.`}>
      <defs><linearGradient id={gradient}><stop offset="0" stopColor="var(--iso-side)" /><stop offset=".45" stopColor="var(--iso-top)" /><stop offset="1" stopColor="var(--iso-side)" /></linearGradient><radialGradient id={`${gradient}-sphere`} cx="32%" cy="25%" r="72%"><stop offset="0" stopColor="var(--iso-top)" /><stop offset="1" stopColor="var(--iso-side)" /></radialGradient></defs>
      {sorted.map(part => <g key={part.index} className={'iso-part ' + (selected === part.index ? 'is-selected' : '')} onClick={() => setActive(part.index)}><Solid part={part} gradient={gradient} /><title>{part.label}</title></g>)}
    </svg>
    <div className="agent-visual-part-list" aria-label="Explore parts">{items.map((part,index) => <button type="button" key={index} aria-pressed={selected === index} onClick={() => setActive(index)}>{part.label}</button>)}</div>
    <div className="agent-visual-detail" aria-live="polite"><strong>{items[selected].label}</strong><p>{items[selected].detail}</p></div>
    <small className="agent-visual-caption">{station ? <><span>Grouped schematic · not to scale</span><a href="https://www.nasa.gov/international-space-station/international-space-station-assembly-elements/" target="_blank" rel="noreferrer">NASA assembly reference</a></> : 'Conceptual isometric view · not to scale'}</small>
  </figure>;
}

function MapGlyph({ symbol }) {
  const paths = {
    module: 'M3 7 12 2 21 7v10l-9 5-9-5Z M3 7l9 5 9-5 M12 12v10',
    storage: 'M4 6c0-5 16-5 16 0s-16 5-16 0v12c0 5 16 5 16 0V6 M4 12c0 5 16 5 16 0',
    processor: 'M5 5h14v14H5Z M9 9h6v6H9Z M8 2v3 M16 2v3 M8 19v3 M16 19v3 M2 8h3 M2 16h3 M19 8h3 M19 16h3',
    person: 'M8 6a4 4 0 1 0 8 0a4 4 0 1 0-8 0 M4 22v-3c0-8 16-8 16 0v3',
    energy: 'M14 2 4 14h7l-1 8 10-12h-7Z',
    cloud: 'M6 19a5 5 0 0 1-1-10 7 7 0 0 1 13-2 6 6 0 0 1 0 12Z',
  };
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[symbol] || paths.module} /></svg>;
}

export function SystemMap({ label, nodes, links }) {
  const [active,setActive] = useState(0);
  const marker = useId().replace(/:/g,'');
  const selected = Math.min(active, nodes.length - 1), current = nodes[selected];
  const rows = Math.ceil(nodes.length / 2), height = rows * 100;
  return <figure className="agent-ui-visual agent-system-map"><figcaption>{label}</figcaption>
    <div className="agent-map-canvas" style={{height}}>
      <svg viewBox={`0 0 600 ${height}`} preserveAspectRatio="none" aria-hidden="true"><defs><marker id={marker} markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0 0 L7 3.5 L0 7Z" fill="currentColor" /></marker></defs>{links.map((link,i) => {
        const a = nodes.findIndex(n => n.id === link.from), b = nodes.findIndex(n => n.id === link.to);
        if (a < 0 || b < 0) return null;
        const ax = a%2 ? 320 : 280, bx = b%2 ? 320 : 280, ay = Math.floor(a/2)*100+50, by = Math.floor(b/2)*100+50;
        const sameSide = a%2 === b%2;
        const sx = sameSide ? (a%2 ? 580 : 20) : ax, tx = sameSide ? (b%2 ? 580 : 20) : bx, lane = sameSide ? (a%2 ? 594 : 6) : 300;
        return <path key={i} className={link.from === current.id || link.to === current.id ? 'is-selected' : ''} d={`M${sx} ${ay} H${lane} V${by} H${tx}`} markerEnd={`url(#${marker})`} />;
      })}</svg>
      {nodes.map((node,i) => <button type="button" key={node.id} className="agent-map-node" style={{left:`${i%2*50+3.333}%`,top:Math.floor(i/2)*100+14}} aria-pressed={selected === i} onClick={() => setActive(i)} title={node.label}><MapGlyph symbol={node.symbol} /><strong>{node.label}</strong></button>)}
    </div>
    <div className="agent-visual-detail" aria-live="polite"><strong>{current.label}</strong><p>{current.detail}</p><ul>{links.filter(link => link.from === current.id || link.to === current.id).map((link,i) => <li key={i}>{nodes.find(n=>n.id===link.from)?.label} → {nodes.find(n=>n.id===link.to)?.label}<small>{link.label}</small></li>)}</ul></div>
    <small className="agent-visual-caption">Relationship map · select a part to trace its connections</small>
  </figure>;
}

export function LineChart({ label, unit, points }) {
  const [active,setActive] = useState(0);
  const low = Math.min(...points.map(p=>p.value)), high = Math.max(...points.map(p=>p.value)), span = high-low || Math.max(1,Math.abs(high)*.1);
  const yMin = high===low ? low-span/2 : low, yMax = high===low ? high+span/2 : high;
  const coords = points.map((p,i)=>[14+i/(points.length-1)*372, 128-(p.value-yMin)/(yMax-yMin)*112]);
  const selected = Math.min(active,points.length-1), point = points[selected];
  return <figure className="agent-ui-visual agent-line-chart"><figcaption>{label}{unit ? ` · ${unit}` : ''}</figcaption><div className="agent-chart-range"><span>{yMax.toLocaleString()}</span><span>{yMin.toLocaleString()}</span></div><svg viewBox="0 0 400 144" role="img" aria-label={`${label}. ${points.length} observations; minimum ${low}, maximum ${high}${unit ? ' '+unit : ''}.`}>
    {[16,72,128].map(y=><path key={y} className="agent-chart-grid" d={`M14 ${y} H386`} />)}<polyline className="agent-chart-line" points={coords.map(p=>p.join(',')).join(' ')} />{coords.map(([x,y],i)=><circle key={i} cx={x} cy={y} r={i===selected?5:3} className={i===selected?'selected':''}><title>{points[i].label}: {points[i].value} {unit}</title></circle>)}
    </svg><div className="agent-chart-categories"><span>{points[0].label}</span><span>{points.at(-1).label}</span></div><div className="agent-chart-controls"><button type="button" disabled={selected===0} onClick={()=>setActive(selected-1)}>Previous</button><output aria-live="polite">{point.label}: <strong>{point.value.toLocaleString()} {unit}</strong></output><button type="button" disabled={selected===points.length-1} onClick={()=>setActive(selected+1)}>Next</button></div></figure>;
}
