import React, { useEffect, useRef, useState } from 'react';

const faces = [
  { corners: [4, 5, 6, 7], normal: [0, 0, 1], shade: 'top' },
  { corners: [1, 2, 6, 5], normal: [1, 0, 0], shade: 'side' },
  { corners: [0, 4, 7, 3], normal: [-1, 0, 0], shade: 'side' },
  { corners: [2, 3, 7, 6], normal: [0, 1, 0], shade: 'front' },
  { corners: [0, 1, 5, 4], normal: [0, -1, 0], shade: 'front' },
];

export function resolveConstructionOps(ops) {
  const byId = new Map();
  return ops.map(op => {
    const source = op.op === 'mirror' ? byId.get(op.source) : null;
    const size = source?.size || op.size;
    const position = source ? [...source.position] : op.position;
    if (source) {
      const axis = op.axis === 'y' ? 1 : 0;
      position[axis] = -position[axis] - size[axis];
    }
    const part = { ...op, position, size };
    byId.set(op.id, part);
    return part;
  });
}

function corners({ position: [x, y, z], size: [w, d, h] }) {
  return [
    [x, y, z], [x + w, y, z], [x + w, y + d, z], [x, y + d, z],
    [x, y, z + h], [x + w, y, z + h], [x + w, y + d, z + h], [x, y + d, z + h],
  ];
}

function draw(parts, degrees) {
  const angle = degrees * Math.PI / 180;
  const c = Math.cos(angle), s = Math.sin(angle);
  const elevation = .55, rise = Math.sqrt(1 - elevation * elevation);
  const project = ([x, y, z]) => [(-s * x + c * y) * 42, -(elevation * (c * x + s * y) + rise * z) * 42];
  const depth = ([x, y, z]) => c * x + s * y + elevation * z;
  const all = parts.flatMap(corners);
  const projected = all.map(project);
  const xs = projected.map(point => point[0]), ys = projected.map(point => point[1]);
  const left = Math.min(...xs) - 22, top = Math.min(...ys) - 22;
  const width = Math.max(...xs) - left + 22, height = Math.max(...ys) - top + 22;
  const visible = parts.flatMap((part, index) => {
    const points = corners(part);
    return faces.filter(face => face.normal[0] * c + face.normal[1] * s + face.normal[2] * elevation > .001)
      .map(face => ({
        key: `${part.id}-${face.shade}-${face.normal.join('')}`,
        index,
        shade: face.shade,
        depth: face.corners.reduce((sum, corner) => sum + depth(points[corner]), 0) / 4,
        points: face.corners.map(corner => project(points[corner]).join(',')).join(' '),
      }));
  }).sort((a, b) => a.depth - b.depth);
  return { viewBox: `${left} ${top} ${width} ${height}`, visible };
}

export function ConstructionScene({ label, ops }) {
  const [selected, setSelected] = useState(0);
  const [yaw, setYaw] = useState(38);
  const pointer = useRef(null);
  const yawFrame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(yawFrame.current), []);
  const parts = resolveConstructionOps(ops);
  const scene = draw(parts, yaw);
  const current = parts[Math.min(selected, parts.length - 1)];
  const endPointer = () => {
    if (yawFrame.current) cancelAnimationFrame(yawFrame.current);
    yawFrame.current = 0;
    if (pointer.current) setYaw(pointer.current.nextYaw);
    pointer.current = null;
  };
  return <figure className="agent-ui-visual agent-construction">
    <figcaption>{label}</figcaption>
    <svg className="agent-construction-canvas" viewBox={scene.viewBox} role="img" aria-label={`${label}. Rotatable schematic of ${parts.length} selectable parts.`}
      onPointerDown={event => { if (event.target.dataset.partIndex != null) setSelected(Number(event.target.dataset.partIndex)); pointer.current = { x: event.clientX, yaw, nextYaw: yaw }; event.currentTarget.setPointerCapture(event.pointerId); }}
      onPointerMove={event => {
        if (!pointer.current) return;
        pointer.current.nextYaw = pointer.current.yaw + (event.clientX - pointer.current.x) * .55;
        if (!yawFrame.current) yawFrame.current = requestAnimationFrame(() => { yawFrame.current = 0; if (pointer.current) setYaw(pointer.current.nextYaw); });
      }}
      onPointerUp={endPointer} onPointerCancel={endPointer} onLostPointerCapture={endPointer}>
      {scene.visible.map(face => <polygon key={face.key} data-part-index={face.index} points={face.points} className={`construction-face shade-${face.shade}${selected === face.index ? ' is-selected' : ''}`}><title>{parts[face.index].label}</title></polygon>)}
    </svg>
    <div className="agent-construction-controls" aria-label="Rotate view"><button type="button" onClick={() => setYaw(value => value - 45)} aria-label="Rotate left">↶</button><span>Drag to rotate</span><button type="button" onClick={() => setYaw(value => value + 45)} aria-label="Rotate right">↷</button></div>
    <div className="agent-visual-part-list" aria-label="Explore parts">{parts.map((part, index) => <button type="button" key={part.id} aria-pressed={selected === index} onClick={() => setSelected(index)}>{part.label}</button>)}</div>
    <div className="agent-visual-detail" aria-live="polite"><strong>{current.label}</strong><p>{current.detail}</p></div>
    <small className="agent-visual-caption">Conceptual construction · not to scale</small>
  </figure>;
}
