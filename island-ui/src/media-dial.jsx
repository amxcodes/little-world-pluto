import React, { useId, useRef } from 'react';

const point = (angle, radius = 50) => {
  const radians = angle * Math.PI / 180;
  return [60 + radius * Math.cos(radians), 60 + radius * Math.sin(radians)];
};
const arc = fraction => {
  const start = point(135), end = point(135 + fraction * 270);
  return `M ${start.join(' ')} A 50 50 0 ${fraction > 2 / 3 ? 1 : 0} 1 ${end.join(' ')}`;
};

// A 270-degree dial: pointer capture keeps dragging stable outside the ring.
export function MediaDial({ value, duration, disabled, variant = 'standard', onPreview, onCommit, children }) {
  const drag = useRef(null);
  const gradient = useId();
  const fraction = duration > 0 ? Math.max(0, Math.min(1, value / duration)) : 0;
  const readPointer = event => {
    const box = event.currentTarget.getBoundingClientRect();
    const angle = (Math.atan2(event.clientY - box.top - box.height / 2, event.clientX - box.left - box.width / 2) * 180 / Math.PI - 135 + 360) % 360;
    const tileColumn = Math.max(0, Math.min(4, Math.floor((event.clientX - box.left) / box.width * 5)));
    const tileRow = Math.max(0, Math.min(4, Math.floor((event.clientY - box.top) / box.height * 5)));
    const position = variant === 'mosaic' ? (tileRow * 5 + tileColumn) / 24 : (angle > 315 ? 0 : Math.min(270, angle)) / 270;
    const next = Math.round(position * duration);
    drag.current = next;
    onPreview(next);
    return next;
  };
  const keyDown = event => {
    const delta = { ArrowRight: 5000, ArrowUp: 5000, ArrowLeft: -5000, ArrowDown: -5000, PageUp: 30000, PageDown: -30000 }[event.key];
    if (disabled || (!Number.isFinite(delta) && event.key !== 'Home' && event.key !== 'End')) return;
    event.preventDefault();
    onCommit(Math.max(0, Math.min(duration, event.key === 'Home' ? 0 : event.key === 'End' ? duration : value + delta)));
  };
  return <div className={`media-dial media-dial-${variant}`}>
    <svg viewBox="0 0 120 120" shapeRendering="geometricPrecision" role="slider" tabIndex={disabled ? -1 : 0} aria-label="Seek in current media" aria-disabled={disabled} aria-valuemin={0} aria-valuemax={Math.round(duration / 1000)} aria-valuenow={Math.round(value / 1000)} aria-valuetext={`${Math.floor(value / 60000)} minutes ${Math.floor(value / 1000) % 60} seconds`} onKeyDown={keyDown}
      onPointerDown={event => { if (disabled || event.button !== 0) return; event.currentTarget.setPointerCapture(event.pointerId); readPointer(event); }}
      onPointerMove={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) readPointer(event); }}
      onPointerUp={event => { if (drag.current == null) return; onCommit(readPointer(event)); drag.current = null; event.currentTarget.releasePointerCapture(event.pointerId); }}
      onPointerCancel={() => { drag.current = null; onPreview(null); }}
      onLostPointerCapture={() => { if (drag.current != null) { drag.current = null; onPreview(null); } }}>
      <defs><linearGradient id={gradient} x1="10" y1="110" x2="110" y2="10" gradientUnits="userSpaceOnUse"><stop stopColor="var(--text)" /><stop offset="1" stopColor="var(--accent)" /></linearGradient><linearGradient id={`${gradient}-tile`} x1="0" y1="0" x2="1" y2="1"><stop stopColor="var(--accent-a)"/><stop offset=".55" stopColor="var(--accent)"/><stop offset="1" stopColor="var(--accent-b)"/></linearGradient></defs>
      {variant === 'mosaic' ? Array.from({ length: 25 }, (_, index) => <rect key={index} className="dial-tile" x={3 + index % 5 * 23} y={3 + Math.floor(index / 5) * 23} width="22" height="22" rx="6" style={{ fill: `url(#${gradient}-tile)`, opacity: fraction > 0 && index / 24 <= fraction ? .65 + (index % 4) * .1 : .12 }} />) : variant === 'segments' ? Array.from({ length: 32 }, (_, index) => {
        const [x1, y1] = point(135 + index / 31 * 270, 43), [x2, y2] = point(135 + index / 31 * 270, 54);
        return <line key={index} className={`dial-segment ${fraction > 0 && index / 31 <= fraction ? 'is-filled' : ''}`} x1={x1} y1={y1} x2={x2} y2={y2} />;
      }) : <>
        {variant === 'halo' && Array.from({ length: 64 }, (_, index) => {
          const [x1, y1] = point(135 + index / 63 * 270, 43), [x2, y2] = point(135 + index / 63 * 270, 47);
          return <line key={index} className={`dial-tick ${fraction > 0 && index / 63 <= fraction ? 'is-filled' : ''}`} x1={x1} y1={y1} x2={x2} y2={y2} />;
        })}
        <path className="dial-track" d={arc(1)} />
        {fraction > 0 && <path className="dial-fill" d={arc(fraction)} style={{ stroke: `url(#${gradient})` }} />}
        <circle className="dial-tip" cx={point(135 + fraction * 270)[0]} cy={point(135 + fraction * 270)[1]} r="3.8" />
      </>}
    </svg>
    <div className="dial-center">{children}</div>
    <span className="dial-elapsed" aria-hidden="true">{Math.floor(value / 60000)}:{String(Math.floor(value / 1000) % 60).padStart(2, '0')}</span>
  </div>;
}
