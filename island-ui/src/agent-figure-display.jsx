import React, { useEffect, useMemo, useRef, useState } from 'react';
import { figureCueAt, figureLoopProgress } from './figure-playback.js';
import { createSceneViewer } from './agent-scene-viewer.js';
import { figureMotionBounds, figurePose, figureReviewDuration, hasPeriodicMotion, motionActiveAt } from './figure-motion.js';

export function SceneDisplay({ label, parts, previewParts = parts, labels, upAxis = 'z', visualRevision, visualStatus, buildStatus = '', onVisualPreview, stageSeconds = .8, motionStage = 0, motionLast = 0, motionPlaying = false, camera, playback, cues = [], showLabels = true, autoOrbit = false, wheelZoom = false, onPause }) {
  const firstSubjectPart = parts.find(part => !/display (?:base|stand|platform)|support (?:plane|platform)|mounting base|pedestal|plinth/i.test(`${part.label} ${part.detail}`)) || parts[0];
  const [selectedId, setSelectedId] = useState(firstSubjectPart?.id || '');
  const [focusSelected, setFocusSelected] = useState(false);
  const [meshViewerUnavailable, setMeshViewerUnavailable] = useState(false);
  const [nativeView, setNativeView] = useState(() => Boolean(window.ipc && !window.islandDetachedViewer));
  const [nativeUnavailable, setNativeUnavailable] = useState(false);
  const focusRef = useRef(focusSelected);
  focusRef.current = focusSelected;
  const canvasRef = useRef(null), pointerRef = useRef(null), frameRef = useRef(0);
  const viewerRef = useRef(null), viewerUnavailableRef = useRef(false), pitchRef = useRef(.48);
  const periodicTime = useRef(0);
  const holdStart = playback?.hold_start ?? 0, holdEnd = playback?.hold_end ?? 0;
  const returnSeconds = playback?.return_seconds;
  const loopPlayback = useMemo(() => returnSeconds == null ? null : {
    hold_start: holdStart, hold_end: holdEnd, return_seconds: returnSeconds,
  }, [holdStart, holdEnd, returnSeconds]);
  const hasPeriodic = Boolean(loopPlayback) || previewParts.some(hasPeriodicMotion);
  const [caption, setCaption] = useState('');
  const captionRef = useRef('');
  const cuesRef = useRef(cues);
  cuesRef.current = cues;
  const updateCaption = stage => {
    const next = figureCueAt(cuesRef.current, motionLast ? stage / motionLast : 0);
    if (next !== captionRef.current) { captionRef.current = next; setCaption(next); }
  };
  const motionTime = useRef(motionStage);
  const motionTarget = useRef(motionStage);
  const hasMotion = previewParts.some(part => part.motion || part.parent_local);
  const motionBounds = useMemo(() => hasMotion ? figureMotionBounds(previewParts, motionLast) : null,
    [hasMotion, previewParts, motionLast]);
  const yawRef = useRef(36), zoomRef = useRef(1), selectedRef = useRef(selectedId), drawRef = useRef(null);
  selectedRef.current = selectedId;
  const draw = (publishNative = true) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    if (!publishNative && viewerRef.current?.isNativePresenting()) return;
    if (!viewerRef.current && !viewerUnavailableRef.current) {
      try { viewerRef.current = createSceneViewer(canvas, upAxis); }
      catch {
        viewerUnavailableRef.current = true;
        setMeshViewerUnavailable(true);
      }
    }
    if (viewerRef.current) {
      viewerRef.current.setNative(nativeView, () => {
        setNativeView(false);
        setNativeUnavailable(true);
      }, () => drawRef.current?.());
      viewerRef.current.draw(parts, yawRef.current, selectedRef.current,
        pitchRef.current, null, focusSelected, hasMotion ? figurePose(parts, motionTime.current, periodicTime.current) : null, motionBounds,
        hasMotion ? { parts, stage: motionTime.current, elapsed: periodicTime.current, target: motionStage, stageSeconds, loopPlayback, loopLastStage: motionLast,
          playing: motionPlaying && !window.matchMedia('(prefers-reduced-motion: reduce)').matches &&
            canvas.closest('.island')?.dataset.reducedMotion !== 'true' } : null, publishNative, labels, showLabels, zoomRef.current);
    }
  };
  drawRef.current = draw;

  useEffect(() => {
    if (!hasMotion) return undefined;
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    const island = canvasRef.current?.closest('.island');
    const reduced = () => media.matches || island?.dataset.reducedMotion === 'true';
    const changedStage = motionTarget.current !== motionStage;
    motionTarget.current = motionStage;
    if (reduced() || (!loopPlayback && ((changedStage && !motionPlaying) ||
      Math.abs(motionStage - motionTime.current) > 1.01))) {
      motionTime.current = motionStage;
      if (motionStage === 0 && changedStage) periodicTime.current = 0;
      drawRef.current?.();
    }
    updateCaption(hasMotion ? motionTime.current : motionStage);
    drawRef.current?.();
    if (!motionPlaying || reduced()) return undefined;
    const start = motionTime.current;
    const duration = 1000 * stageSeconds * Math.abs(motionStage - start);
    let frame, elapsed = 0, previous;
    const needsFrame = () => Boolean(loopPlayback) || previewParts.some(part => motionActiveAt(part, periodicTime.current)) || elapsed < duration;
    const tick = now => {
      if (reduced()) {
        motionTime.current = motionStage;
        periodicTime.current = 0;
        drawRef.current?.();
        return;
      }
      if (previous != null) {
        const delta = Math.min(40, now - previous);
        elapsed += delta;
        if (hasPeriodic) periodicTime.current += delta / 1000;
        const progress = duration ? Math.min(1, elapsed / duration) : 1;
        motionTime.current = loopPlayback
          ? motionLast * figureLoopProgress(loopPlayback, stageSeconds * motionLast, periodicTime.current)
          : start + (motionStage - start) * progress;
        updateCaption(motionTime.current);
        drawRef.current?.(false);
      }
      previous = now;
      if (needsFrame()) frame = requestAnimationFrame(tick);
    };
    const resume = () => {
      cancelAnimationFrame(frame);
      previous = undefined;
      if (!document.hidden && needsFrame()) frame = requestAnimationFrame(tick);
    };
    document.addEventListener('visibilitychange', resume);
    resume();
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('visibilitychange', resume);
    };
  }, [motionStage, motionPlaying, hasMotion, hasPeriodic, previewParts, stageSeconds, loopPlayback, motionLast]);

  useEffect(() => {
    if (!parts.some(part => part.id === selectedId)) {
      const subjectPart = parts.find(part => !/display (?:base|stand|platform)|support (?:plane|platform)|mounting base|pedestal|plinth/i.test(`${part.label} ${part.detail}`)) || parts[0];
      setSelectedId(subjectPart?.id || '');
    }
  }, [parts, selectedId]);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const resize = new ResizeObserver(() => drawRef.current?.());
    resize.observe(canvas);
    const root = canvas.closest('.island');
    const theme = root && new MutationObserver(() => drawRef.current?.());
    theme?.observe(root, { attributes: true, attributeFilter: ['class', 'data-theme', 'data-material'] });
    drawRef.current?.();
    return () => { resize.disconnect(); theme?.disconnect(); if (frameRef.current) cancelAnimationFrame(frameRef.current); };
  }, [parts]);
  useEffect(() => () => { viewerRef.current?.dispose(); viewerRef.current = null; }, []);
  useEffect(() => { drawRef.current?.(); }, [selectedId, focusSelected, nativeView, labels, showLabels]);
  const cameraYaw = camera?.yaw, cameraPitch = camera?.pitch;
  const orbitSpeed = camera?.orbit_degrees_per_second ?? 12;
  useEffect(() => {
    if (cameraYaw != null) yawRef.current = cameraYaw;
    if (cameraPitch != null) pitchRef.current = cameraPitch * Math.PI / 180;
    drawRef.current?.();
  }, [cameraYaw, cameraPitch]);
  useEffect(() => {
    updateCaption(hasMotion ? motionTime.current : motionStage);
  }, [hasMotion, motionStage, motionLast, cues]);
  useEffect(() => {
    if (!autoOrbit || !orbitSpeed || window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
      canvasRef.current?.closest('.island')?.dataset.reducedMotion === 'true') return undefined;
    let frame = 0, previous = 0, painted = 0;
    const tick = now => {
      if (previous) yawRef.current = (yawRef.current + Math.min(40, now - previous) * orbitSpeed / 1000 + 360) % 360;
      previous = now;
      if (now - painted >= 32) { painted = now; drawRef.current?.(); }
      frame = requestAnimationFrame(tick);
    };
    const visibility = () => {
      cancelAnimationFrame(frame);
      previous = 0;
      if (!document.hidden) frame = requestAnimationFrame(tick);
    };
    document.addEventListener('visibilitychange', visibility);
    visibility();
    return () => { cancelAnimationFrame(frame); document.removeEventListener('visibilitychange', visibility); };
  }, [autoOrbit, orbitSpeed]);
  const previewCallback = useRef(onVisualPreview);
  previewCallback.current = onVisualPreview;
  const displayedParts = useRef(parts);
  displayedParts.current = parts;
  useEffect(() => {
    if (!visualStatus || !visualRevision || !previewCallback.current) return undefined;
    const timer = setTimeout(() => {
      if (!viewerRef.current) drawRef.current?.();
      const canvas = canvasRef.current;
      if (!canvas?.width || !canvas?.height) return;
      if (viewerRef.current) {
        const reviewEnd = motionLast || Number(hasPeriodic);
        const duration = loopPlayback ? motionLast * stageSeconds : figureReviewDuration(previewParts, motionLast, stageSeconds);
        const image = viewerRef.current.preview(previewParts, focusRef.current, displayedParts.current,
          hasMotion ? figurePose(previewParts, reviewEnd, holdStart + duration) : null, reviewEnd,
          hasMotion && reviewEnd > 0 ? figurePose(previewParts, 0, holdStart) : null,
          reviewEnd ? duration / reviewEnd : stageSeconds, motionBounds, holdStart);
        if (image) previewCallback.current(visualRevision, image);
        return;
      }
    }, 100);
    return () => clearTimeout(timer);
  }, [previewParts, visualRevision, visualStatus, hasMotion, hasPeriodic, motionLast, stageSeconds, motionBounds, labels, showLabels, loopPlayback, holdStart]);

  const onPointerDown = event => {
    if (event.button !== 0) return;
    onPause?.();
    pointerRef.current = { x: event.clientX, y: event.clientY, lastX: event.clientX, lastY: event.clientY, moved: false };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = event => {
    const pointer = pointerRef.current;
    if (!pointer) return;
    const delta = event.clientX - pointer.lastX;
    const deltaY = event.clientY - pointer.lastY;
    pointer.lastX = event.clientX;
    pointer.lastY = event.clientY;
    if (Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) > 3) pointer.moved = true;
    if (!pointer.moved || (!delta && !deltaY)) return;
    yawRef.current = (yawRef.current + delta * .55 + 360) % 360;
    pitchRef.current = Math.max(-1.25, Math.min(1.35, pitchRef.current + deltaY * .007));
    if (!frameRef.current) frameRef.current = requestAnimationFrame(() => { frameRef.current = 0; drawRef.current?.(); });
  };
  const onPointerUp = event => {
    const pointer = pointerRef.current;
    if (!pointer) return;
    pointerRef.current = null;
    if (!pointer.moved) {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      // Native playback owns each frame; refresh the pick geometry on demand.
      drawRef.current?.();
      const hit = viewerRef.current?.pick(x, y);
      if (hit) setSelectedId(hit);
    }
  };
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !wheelZoom) return undefined;
    const zoom = event => {
      event.preventDefault();
      onPause?.();
      zoomRef.current = Math.max(.65, Math.min(2.5, zoomRef.current * Math.exp(-event.deltaY * .001)));
      drawRef.current?.();
    };
    canvas.addEventListener('wheel', zoom, { passive: false });
    return () => canvas.removeEventListener('wheel', zoom);
  }, [wheelZoom, onPause, parts.length]);
  const onDoubleClick = event => {
    const rect = event.currentTarget.getBoundingClientRect();
    const hit = viewerRef.current?.pick(event.clientX - rect.left, event.clientY - rect.top);
    if (hit) { setSelectedId(hit); setFocusSelected(value => selectedRef.current === hit ? !value : true); }
    else setFocusSelected(false);
  };  if (!parts.length) return null;
  return <figure className="agent-ui-visual agent-procedural-scene">
    <figcaption><span>{label}</span></figcaption>
    {cues.length > 0 && <p className="agent-scene-caption" aria-live="polite">{caption}</p>}
    {buildStatus && <small className="agent-scene-build-status" role="status" aria-live="polite">{buildStatus}</small>}
    {meshViewerUnavailable ? <p role="status">The 3D viewer is unavailable on this device.</p> :
      <canvas ref={canvasRef} className="agent-procedural-canvas" role="img" aria-label={`${label}. Interactive 3D construction with ${parts.length} labeled parts. Drag to orbit, double-click to focus a part, and use the model scroll button to enable wheel zoom. Click to pause animation.`} onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onPointerCancel={() => { pointerRef.current = null; }} onDoubleClick={onDoubleClick} />}
  </figure>;
}
