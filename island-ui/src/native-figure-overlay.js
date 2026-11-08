import { figureBindMatrix } from './figure-motion.js';
import { Matrix4, Vector3 } from 'three';
import { figureAnnotations } from './figure-annotations.js';

let nextId = 0;

function encode(array) {
  const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
  let binary = '';
  for (let i = 0; i < bytes.length; i += 8192) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  }
  return btoa(binary);
}

export function createNativeFigureOverlay(canvas, onFailure, onChange) {
  const id = ++nextId;
  const send = (kind, data = {}) => window.ipc?.postMessage(JSON.stringify({ kind, id, ...data }));
  let known = new WeakMap();
  let nextMesh = 0, pending = 0, latest = null, alive = true;
  let active = false, retries = 0, lastRetry = 0;
  const result = canvas.closest?.('.agent-result');
  const historyObscures = () => Boolean(result?.querySelector('.agent-turn-history'));
  const clips = [];
  for (let parent = canvas.parentElement; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (/(hidden|clip|scroll|auto)/.test(style.overflowX + ' ' + style.overflowY)) clips.push(parent);
  }
  const visible = rect => !historyObscures() && !document.hidden && canvas.isConnected && rect.width >= 2 &&
    rect.height >= 2 && rect.left >= 0 && rect.top >= 0 &&
    rect.right <= innerWidth + 1 && rect.bottom <= innerHeight + 1 &&
    clips.every(parent => {
      const clip = parent.getBoundingClientRect();
      return rect.left >= clip.left - 1 && rect.right <= clip.right + 1 &&
        rect.top >= clip.top - 1 && rect.bottom <= clip.bottom + 1;
    });
  const notifyFailure = failed => {
    if (failed === id && alive) onFailure();
  };
  const presenting = (owner, value) => {
    if (owner !== id || !alive || value === active) return;
    active = value;
    onChange();
  };
  const hide = () => {
    retries = 0;
    send('nativeFigureHide');
    presenting(id, false);
  };
  window.islandNativeFigureFailed = notifyFailure;
  window.islandNativeFigurePresenting = presenting;
  send('nativeFigureStart');

  function flush() {
    pending = 0;
    if (!alive || !latest) return;
    const rect = canvas.getBoundingClientRect();
    if (!visible(rect)) {
      hide();
      return;
    }
    const { meshes, camera, timeline, selectedId, labels, showLabels } = latest;
    const nodeIndices = new Map(timeline?.parts.map((part, index) => [part.id, index]));
    const motion = timeline && {
      stage: timeline.stage,
      target: timeline.target,
      stage_seconds: timeline.stageSeconds,
      playing: timeline.playing,
      elapsed: timeline.elapsed,
      loop_playback: timeline.loopPlayback,
      loop_last_stage: timeline.loopLastStage,
      nodes: timeline.parts.map(part => ({
        parent: nodeIndices.get(part.parent),
        step: part.step,
        motion: part.motion,
        parent_local: !!part.parent_local,
        bind: figureBindMatrix(part).elements,
      })),
    };
    if (!meshes.length || meshes.some(mesh => !mesh.userData.part.positions)) {
      hide();
      return;
    }
    const newMeshes = new Set(meshes.map(mesh => mesh.userData.part.positions).filter(positions => !known.has(positions)));
    if (nextMesh + newMeshes.size > 140) {
      send('nativeFigureStart');
      presenting(id, false);
      known = new WeakMap();
      nextMesh = 0;
    }
    let uploads = [], uploadBytes = 0;
    const sendUploads = () => {
      if (uploads.length) send('nativeFigureMeshes', { meshes: uploads });
      uploads = [];
      uploadBytes = 0;
    };
    const draws = meshes.map(mesh => {
      const { positions, normals, indices } = mesh.userData.part;
      let meshId = known.get(positions);
      if (!meshId) {
        meshId = ++nextMesh;
        const vertices = new Float32Array(positions.length * 2);
        for (let i = 0; i < positions.length; i += 3) {
          vertices.set(positions.subarray(i, i + 3), i * 2);
          vertices.set(normals.subarray(i, i + 3), i * 2 + 3);
        }
        const upload = { id: meshId, vertices: encode(vertices), indices: encode(indices) };
        const bytes = upload.vertices.length + upload.indices.length;
        if (uploads.length >= 24 || uploadBytes + bytes > 4_000_000) sendUploads();
        uploads.push(upload);
        uploadBytes += bytes;
        known.set(positions, meshId);
      }
      const color = (mesh.userData.partColor || mesh.material.color).clone().convertLinearToSRGB();
      return { id: meshId, node: nodeIndices.get(mesh.userData.part.id), pose: mesh.matrixWorld.toArray(), color: [color.r, color.g, color.b, 0],
        finish: [mesh.material.roughness, mesh.material.metalness],
        grain: { brushed: 1, grained: 2, matte: 3 }[mesh.userData.part.finish],
        opacity: mesh.material.opacity, center: mesh.userData.part.bounds?.center };
    });
    sendUploads();
    const annotations = showLabels ? figureAnnotations(labels, timeline?.parts ?? meshes.map(mesh => mesh.userData.part), selectedId) : [];
    send('nativeFigureFrame', { frame: {
      rect: [rect.left, rect.top, rect.width, rect.height],
      viewDirection: camera.getWorldDirection(new Vector3()).negate().toArray(),
      viewProjection: new Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse).toArray(),
      draws,
      timeline: motion,
      labels: annotations.map(label => ({ text: label.text, point: label.point, node: nodeIndices.get(label.part) })),
    } });
  }
  const schedule = () => { if (alive && !pending) pending = requestAnimationFrame(flush); };
  const historyObserver = result && typeof MutationObserver !== 'undefined' ? new MutationObserver(schedule) : null;
  historyObserver?.observe(result, { childList: true });
  const retry = owner => {
    if (owner !== id || !alive) return;
    const now = performance.now();
    if (now - lastRetry > 250) retries = 0;
    lastRetry = now;
    if (++retries > 12) onFailure();
    else schedule();
  };
  window.islandNativeFigureRetry = retry;
  const visibilityChanged = () => document.hidden ? hide() : schedule();
  window.addEventListener('scroll', schedule, true);
  window.addEventListener('resize', schedule);
  document.addEventListener('visibilitychange', visibilityChanged);
  return {
    isPresenting: () => active,
    draw(meshes, camera, timeline, selectedId, labels, showLabels = true) {
      latest = { meshes, camera, timeline, selectedId, labels, showLabels };
      schedule();
    },
    dispose() {
      alive = false;
      cancelAnimationFrame(pending);
      historyObserver?.disconnect();
      window.removeEventListener('scroll', schedule, true);
      window.removeEventListener('resize', schedule);
      document.removeEventListener('visibilitychange', visibilityChanged);
      if (window.islandNativeFigureFailed === notifyFailure) delete window.islandNativeFigureFailed;
      if (window.islandNativeFigurePresenting === presenting) delete window.islandNativeFigurePresenting;
      if (window.islandNativeFigureRetry === retry) delete window.islandNativeFigureRetry;
      send('nativeFigureClose');
    },
  };
}
