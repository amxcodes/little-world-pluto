import React, { useEffect, useRef } from 'react';
import { rangeEdge, rangeValueAt } from './split-range-geometry.js';

export function SplitRange({ label, value, min = 0, max = 100, step = 1, unit = '%', disabled = false, id, onChange, className = '', ...events }) {
  const inputRef = useRef(null);
  const dragging = useRef(false);
  const drag = useRef({ frame:0, value:null, last:null, bounds:null, hyper:false, resize:null, abort:null, owner:null, pointerId:null });
  const change = useRef(onChange);change.current=onChange;
  const { onPointerDown, onPointerUp, onPointerCancel, onKeyDown, onBlur, ...inputEvents } = events;
  const cancel = useRef(onPointerCancel);cancel.current=onPointerCancel;
  const number = Number(value);
  const progress = Math.max(0, Math.min(100, (number - min) / Math.max(1, max - min) * 100));
  const edge = rangeEdge(progress);
  const emit = next => {
    if(drag.current.hyper&&drag.current.last===next)return;
    drag.current.last=next;
    change.current?.({ target: { value: String(next) }, currentTarget: { value: String(next) } });
  };
  const flush = () => {
    const state=drag.current;
    if(state.frame)cancelAnimationFrame(state.frame);state.frame=0;
    if(state.value!=null){const next=state.value;state.value=null;emit(next);}
  };
  const release = () => {
    flush();dragging.current=false;
    if(drag.current.resize)window.removeEventListener('resize',drag.current.resize);
    if(drag.current.abort)window.removeEventListener('blur',drag.current.abort);
    const {owner,pointerId}=drag.current;
    if(owner?.hasPointerCapture(pointerId))owner.releasePointerCapture(pointerId);
    drag.current.resize=null;drag.current.abort=null;drag.current.bounds=null;drag.current.owner=null;
  };
  useEffect(()=>()=>{
    if(drag.current.frame)cancelAnimationFrame(drag.current.frame);
    if(drag.current.resize)window.removeEventListener('resize',drag.current.resize);
    if(drag.current.abort)window.removeEventListener('blur',drag.current.abort);
  },[]);
  const updatePointer = (event, immediate=false) => {
    const state=drag.current;
    // Hyper Glass keeps the layout slot still while its face deforms. Cache
    // that slot per gesture, rather than flushing layout on every move.
    const bounds = state.hyper ? (state.bounds ||= event.currentTarget.getBoundingClientRect()) : event.currentTarget.getBoundingClientRect();
    if (!bounds.width) return;
    const next = rangeValueAt(event.clientX, bounds.left, bounds.width, min, max, step);
    if(!state.hyper){emit(next);return;}
    state.value=next;
    if(immediate){flush();return;}
    // Update the React tree at most once per frame, preserving the last value.
    if(!state.frame)state.frame=requestAnimationFrame(()=>{state.frame=0;flush();});
  };
  return <div className={'split-range ' + className} data-disabled={disabled} style={{ '--split-progress': `${progress}%`, '--split-gap': `${10 * edge}px`, '--split-divider-width': `${2 + 4 * edge}px`, '--split-edge-inset': `${5 * (1 - edge)}px` }}
    onPointerDown={event => {
      if (disabled) return;
      event.preventDefault();dragging.current=true;
      const state=drag.current;state.hyper=!!event.currentTarget.closest('[data-material="hyper-glass"]');state.last=null;state.bounds=null;
      if(state.hyper){event.currentTarget.dataset.rangePointer='';state.owner=event.currentTarget;state.pointerId=event.pointerId;state.resize=()=>{state.bounds=null;};state.abort=e=>{if(dragging.current){release();cancel.current?.(e);}};window.addEventListener('resize',state.resize);window.addEventListener('blur',state.abort);}
      event.currentTarget.setPointerCapture(event.pointerId);inputRef.current?.focus({preventScroll:true});onPointerDown?.(event);updatePointer(event,true);
    }}
    onPointerMove={event => { if (dragging.current) { if(drag.current.hyper&&!(event.buttons&1)){release();cancel.current?.(event);}else updatePointer(event); } }}
    onPointerUp={event => { if (!dragging.current) return; updatePointer(event,true);release();if(event.currentTarget.hasPointerCapture(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);onPointerUp?.(event); }}
    onPointerCancel={event => { release();onPointerCancel?.(event); }}>
    <div className="split-range-face" aria-hidden="true">
      <span className="split-range-filled"><strong>{label} {number}{unit}</strong></span>
      <i className="split-range-divider" />
      <span className="split-range-remaining"><strong>{Math.max(0, max - number)}{unit} LEFT</strong></span>
    </div>
    <input ref={inputRef} id={id} className="split-range-input" type="range" min={min} max={max} step={step} value={number} disabled={disabled} aria-label={label} aria-valuetext={`${number}${unit}`} onChange={onChange}
      onKeyDown={event=>{event.currentTarget.parentElement.removeAttribute('data-range-pointer');onKeyDown?.(event);}}
      onBlur={event=>{event.currentTarget.parentElement.removeAttribute('data-range-pointer');onBlur?.(event);}} {...inputEvents} />
  </div>;
}
