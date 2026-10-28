import React, { useRef } from 'react';
import { rangeEdge, rangeValueAt } from './split-range-geometry.js';

export function SplitRange({ label, value, min = 0, max = 100, step = 1, unit = '%', disabled = false, id, onChange, className = '', ...events }) {
  const inputRef = useRef(null);
  const dragging = useRef(false);
  const { onPointerDown, onPointerUp, onPointerCancel, ...inputEvents } = events;
  const number = Number(value);
  const progress = Math.max(0, Math.min(100, (number - min) / Math.max(1, max - min) * 100));
  const edge = rangeEdge(progress);
  const updatePointer = event => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (!bounds.width) return;
    const next = rangeValueAt(event.clientX, bounds.left, bounds.width, min, max, step);
    onChange?.({ target: { value: String(next) }, currentTarget: { value: String(next) } });
  };
  return <div className={'split-range ' + className} data-disabled={disabled} style={{ '--split-progress': `${progress}%`, '--split-gap': `${10 * edge}px`, '--split-divider-width': `${2 + 4 * edge}px`, '--split-edge-inset': `${5 * (1 - edge)}px` }}
    onPointerDown={event => { if (disabled) return; event.preventDefault(); dragging.current = true; event.currentTarget.setPointerCapture(event.pointerId); inputRef.current?.focus({ preventScroll: true }); onPointerDown?.(event); updatePointer(event); }}
    onPointerMove={event => { if (dragging.current) updatePointer(event); }}
    onPointerUp={event => { if (!dragging.current) return; updatePointer(event); dragging.current = false; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); onPointerUp?.(event); }}
    onPointerCancel={event => { dragging.current = false; onPointerCancel?.(event); }}>
    <div className="split-range-face" aria-hidden="true">
      <span className="split-range-filled"><strong>{label} {number}{unit}</strong></span>
      <i className="split-range-divider" />
      <span className="split-range-remaining"><strong>{Math.max(0, max - number)}{unit} LEFT</strong></span>
    </div>
    <input ref={inputRef} id={id} className="split-range-input" type="range" min={min} max={max} step={step} value={number} disabled={disabled} aria-label={label} aria-valuetext={`${number}${unit}`} onChange={onChange} {...inputEvents} />
  </div>;
}
