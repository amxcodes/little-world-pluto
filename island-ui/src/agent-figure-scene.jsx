import { figurePoseInspection, figureReviewDuration } from './figure-motion.js';
import { nextFigureLoopPose, transformationPlaybackIssue } from './figure-playback.js';
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { SceneDisplay } from './agent-figure-display.jsx';
import { HugeiconsIcon } from '@hugeicons/react';
import { PlayIcon, PauseIcon, ViewIcon, ViewOffIcon, MouseScroll01Icon } from '@hugeicons/core-free-icons';

export function useStagePlayback(stage, last, playing, setStage, ready = true) {
  useEffect(() => {
    if (!playing || !ready || stage >= last) return undefined;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
      document.querySelector('.island[data-reduced-motion="true"]')) {
      setStage(last);
      return undefined;
    }
    let timer;
    const schedule = () => {
      clearTimeout(timer);
      if (!document.hidden) timer = setTimeout(() => setStage(value => Math.min(last, value + 1)), 1000);
    };
    schedule();
    document.addEventListener('visibilitychange', schedule);
    return () => {
      clearTimeout(timer);
      document.removeEventListener('visibilitychange', schedule);
    };
  }, [playing, ready, stage, last, setStage]);
}

function useFigureLoop(stage, last, playing, setStage, ready, stageSeconds) {
  const direction = useRef(1);
  const started = useRef(false);
  useEffect(() => {
    if (!playing || !ready || last < 1 || window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
      document.querySelector('.island[data-reduced-motion="true"]')) return undefined;
    let timer;
    const advance = () => {
      clearTimeout(timer);
      if (!document.hidden) timer = setTimeout(() => setStage(current => {
        const next = nextFigureLoopPose(current, last, direction.current);
        direction.current = next.direction;
        return next.stage;
      }), stageSeconds ? stageSeconds * 1000 : 1000);
    };
    if (!document.hidden && !started.current && stage === 0) {
      started.current = true;
      setStage(1);
      return undefined;
    }
    advance();
    document.addEventListener('visibilitychange', advance);
    return () => { clearTimeout(timer); document.removeEventListener('visibilitychange', advance); };
  }, [stage, last, playing, ready, setStage, stageSeconds]);
}
export function FigureScene({ scene, live, visualRevision, visualStatus, visualRequest = '', onVisualOutcome, onVisualPreview }) {
  const [build, setBuild] = useState({ parts: [], key: '', done: false, error: '' });
  const [stage, setStage] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [labelsVisible, setLabelsVisible] = useState(true);
  const [wheelZoom, setWheelZoom] = useState(false);
  const workerRef = useRef(null), request = useRef(0), cache = useRef(new Map());
  const previousParts = useRef(new Map());
  const outcome = useRef(onVisualOutcome);
  outcome.current = onVisualOutcome;
  const recipe = JSON.stringify(scene.parts);
  const reviewPlan = JSON.stringify(scene.assembly_plan ?? null);
  const steps = scene.steps ?? [];
  const last = Math.max(0, steps.length - 1);
  const stageSeconds = scene.duration && last ? scene.duration / last : 1;
  const holdStart = scene.playback?.hold_start ?? 0;
  const cues = scene.cues?.length ? scene.cues : steps.flatMap((step, index) =>
    step.caption ? [{ at: last ? index / last : 0, text: step.caption }] : []);
  useEffect(() => () => workerRef.current?.terminate(), []);
  useEffect(() => {
    const id = ++request.current;
    let worker, frame, submission, timeout, hardTimeout;
    const parts = [];
    const nextCache = new Map();
    let failed = false;
    const fail = error => {
      failed = true;
      clearTimeout(timeout);
      clearTimeout(hardTimeout);
      cancelAnimationFrame(frame);
      setBuild(previous => ({ ...previous, error, done: false, key: recipe }));
    };
    try {
      worker = workerRef.current ??= new Worker('/figure-worker.js', { type: 'module' });
      worker.onmessage = ({ data }) => {
        if (data.id !== id || failed) return;
        if (data.error) return fail(data.error);
        // A detailed part can take a while to cook. Measure stalls between
        // geometry results instead of timing out the entire scene at 30s.
        clearTimeout(timeout);
        timeout = setTimeout(() => {
          worker.terminate();
          workerRef.current = null;
          fail('Figure construction stopped making progress for 30 seconds');
        }, 30_000);
        if (data.done) {
          clearTimeout(timeout);
          clearTimeout(hardTimeout);
          cancelAnimationFrame(frame);
          cache.current = nextCache;
          previousParts.current = new Map(parts.map(part => [part.id, part]));
          setBuild({ parts, key: recipe, done: true, error: '' });
          return;
        }
        const { part } = data;
        const mesh = part.positions ? part : nextCache.get(part.key) ?? cache.current.get(part.key);
        if (!mesh && part.shape.kind !== 'group') return fail('Figure mesh was unavailable');
        const cooked = mesh ? { ...part, positions: mesh.positions, normals: mesh.normals,
          indices: mesh.indices, bounds: mesh.bounds } : part;
        if (mesh) {
          nextCache.set(part.key, cooked);
          cache.current.set(part.key, cooked);
        }
        const previous = previousParts.current.get(part.id);
        parts.push(previous?.key === cooked.key && previous.label === cooked.label &&
          previous.detail === cooked.detail && previous.color === cooked.color &&
          previous.material === cooked.material && previous.parent === cooked.parent &&
          previous.parent_local === cooked.parent_local && JSON.stringify(previous.joint) === JSON.stringify(cooked.joint) &&
          previous.step === cooked.step && JSON.stringify(previous.motion) === JSON.stringify(cooked.motion)
          ? previous : cooked);
        if (!frame) frame = requestAnimationFrame(() => {
          frame = 0;
          setBuild(previous => {
            const appendsToPrevious = parts.length > previous.parts.length &&
              previous.parts.every((part, index) => part === parts[index]);
            return previous.done && !appendsToPrevious
              ? previous
              : { parts: [...parts], key: recipe, done: false, error: '' };
          });
        });
      };
      worker.onerror = () => {
        worker.terminate();
        workerRef.current = null;
        fail('Figure worker failed');
      };
      submission = setTimeout(() => {
        try {
          worker.postMessage({
            id, parts: JSON.parse(recipe),
            known: Array.from(cache.current, ([key, part]) => [key, part.indices.length / 3]),
          });
          hardTimeout = setTimeout(() => {
            worker.terminate();
            workerRef.current = null;
            fail('Figure construction exceeded its two-minute safety limit');
          }, 120_000);
          timeout = setTimeout(() => {
            worker.terminate();
            workerRef.current = null;
            fail('Figure construction stopped making progress for 30 seconds');
          }, 30_000);
        } catch (error) {
          fail(String(error.message || error));
        }
      }, 16);
    } catch (error) {
      fail(String(error.message || error));
    }
    return () => {
      clearTimeout(timeout);
      clearTimeout(hardTimeout);
      clearTimeout(submission);
      cancelAnimationFrame(frame);
      if (worker) {
        worker.onmessage = worker.onerror = null;
      }
    };
  }, [recipe]);
  const complete = build.done && build.key === recipe;
  useEffect(() => {
    if (!visualRevision || build.key !== recipe || (!complete && !build.error)) return undefined;
    const timer = setTimeout(() => {
      const inspection = complete ? figurePoseInspection(build.parts, last, JSON.parse(reviewPlan),
        scene.playback ? last * stageSeconds : figureReviewDuration(build.parts, last, stageSeconds), holdStart) : "";
      outcome.current?.(visualRevision, build.error, inspection);
    }, 0);
    return () => clearTimeout(timer);
  }, [build, complete, recipe, visualRevision, reviewPlan, last, stageSeconds, holdStart, Boolean(scene.playback)]);
  const motionIssue = transformationPlaybackIssue(scene, visualRequest);
  // Streamed prefixes can lack motion. Keep the user's play/pause choice.
  const canPlay = complete && !live && !motionIssue;
  const motionPlaying = canPlay && playing;
  useFigureLoop(stage, last, motionPlaying, setStage, canPlay && !scene.playback, stageSeconds);
  const active = motionIssue ? 0 : Math.min(stage, last);
  const displayed = useMemo(() => build.parts.filter(part => part.step <= active), [build.parts, active]);
  const buildStatus = complete ? (live ? 'Incomplete figure preview' : '') : build.key === recipe
    ? `Building figure · ${Math.min(build.parts.length, scene.parts.length)} of ${scene.parts.length} parts`
    : 'Updating figure';
  return <section className="agent-figure">
    {displayed.some(part => part.positions)
      ? <SceneDisplay label={scene.label} parts={displayed} previewParts={build.parts} labels={scene.labels} showLabels={labelsVisible && complete && !live} wheelZoom={wheelZoom} autoOrbit={motionPlaying} camera={scene.camera} playback={scene.playback} cues={cues} onPause={() => setPlaying(false)} upAxis="y"
          visualRevision={visualRevision} visualStatus={complete ? visualStatus : ''} buildStatus={buildStatus}
          stageSeconds={stageSeconds} motionStage={active} motionLast={last} motionPlaying={motionPlaying}
          onVisualPreview={onVisualPreview} />
      : <p role="status">{build.error || buildStatus || 'Building figure…'}</p>}
    {build.error && displayed.length > 0 && <p role="status">{build.error}</p>}
    {complete && !live && motionIssue && <p className="agent-scene-motion-warning" role="status">{motionIssue}</p>}
    {complete && !live && <div className="agent-figure-playback" aria-label="Model playback">
      <button type="button" disabled={!!motionIssue} aria-label={motionIssue || (playing ? 'Pause model animation' : 'Play model animation')} title={motionIssue || (playing ? 'Pause animation' : 'Play animation')} onClick={() => setPlaying(value => !value)}><HugeiconsIcon icon={playing ? PauseIcon : PlayIcon} size={17} /></button>
      <button type="button" aria-label={labelsVisible ? 'Hide model labels' : 'Show model labels'} title={labelsVisible ? 'Hide labels' : 'Show labels'} aria-pressed={labelsVisible} onClick={() => setLabelsVisible(value => !value)}><HugeiconsIcon icon={labelsVisible ? ViewIcon : ViewOffIcon} size={17} /></button>
      <button type="button" aria-label={wheelZoom ? 'Use wheel to scroll results' : 'Use wheel to zoom model'} title={wheelZoom ? 'Scroll results' : 'Zoom model with wheel'} aria-pressed={wheelZoom} onClick={() => setWheelZoom(value => !value)}><HugeiconsIcon icon={MouseScroll01Icon} size={17} /></button>
    </div>}
  </section>;
}
