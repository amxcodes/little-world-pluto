import React, { useId, useState } from 'react';

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
        const sx = sameSide ? (a%2 ? 580 : 20) : ax, tx = sameSide ? (b%2 ? 580 : 20) : bx;
        const reverseIndex = links.findIndex((candidate, candidateIndex) => candidateIndex !== i && candidate.from === link.to && candidate.to === link.from);
        const hasReverse = reverseIndex >= 0;
        const routeOffset = hasReverse ? (i < reverseIndex ? 8 : 22) : 0;
        const lane = sameSide ? (a%2 ? 594 - routeOffset : 6 + routeOffset) : 300;
        const path = hasReverse && !sameSide
          ? `M${sx} ${ay} Q300 ${(ay + by) / 2 + (i < reverseIndex ? -10 : 10)} ${tx} ${by}`
          : `M${sx} ${ay} H${lane} V${by} H${tx}`;
        return <path key={i} className={link.from === current.id || link.to === current.id ? 'is-selected' : ''} d={path} markerEnd={`url(#${marker})`} />;
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
