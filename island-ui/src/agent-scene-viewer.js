import { figureAppearance, figurePose, figureVisibility } from './figure-motion.js';
import {
  AmbientLight, Box3, BufferAttribute, BufferGeometry, Color, DirectionalLight,
  DataTexture, DoubleSide, EdgesGeometry, Group, LineBasicMaterial, LineSegments,
  LinearFilter, Mesh, MeshStandardMaterial, OrthographicCamera, Raycaster, RepeatWrapping,
  RGBAFormat, Scene, SRGBColorSpace,
  Vector2, Vector3, WebGLRenderer,
} from 'three';
import { createNativeFigureOverlay } from './native-figure-overlay.js';
import { createFigureAnnotations, figureAnnotations } from './figure-annotations.js';

const MAX_PIXEL_RATIO = 1.5;
const BACKGROUND = [22, 23, 29];

function cssRgb(value, fallback) {
  const match = value?.match(/#([\da-f]{6})\b/i);
  if (match) return [0, 2, 4].map(index => parseInt(match[1].slice(index, index + 2), 16));
  const channels = value?.match(/\d+(?:\.\d+)?/g)?.slice(0, 3).map(Number);
  return channels?.length === 3 ? channels : fallback;
}

function colorFromRgb(rgb) {
  return new Color().setRGB(rgb[0] / 255, rgb[1] / 255, rgb[2] / 255, SRGBColorSpace);
}

function geometryForPart(part) {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(part.positions, 3));
  geometry.setAttribute('normal', new BufferAttribute(part.normals, 3));
  geometry.setIndex(new BufferAttribute(part.indices, 1));
  if (part.finish && part.finish !== 'smooth') {
    const uvs = new Float32Array(part.positions.length / 3 * 2);
    for (let i = 0, j = 0; i < part.positions.length; i += 3, j += 2) {
      const nx = Math.abs(part.normals[i]), ny = Math.abs(part.normals[i + 1]);
      const nz = Math.abs(part.normals[i + 2]);
      const [u, v] = nx > ny && nx > nz ? [2, 1] : ny > nz ? [0, 2] : [0, 1];
      uvs[j] = 2 * part.positions[i + u];
      uvs[j + 1] = 2 * part.positions[i + v];
    }
    geometry.setAttribute('uv', new BufferAttribute(uvs, 2));
  }
  geometry.computeBoundingBox();
  return geometry;
}

function disposeMesh(mesh) {
    mesh.geometry.dispose();
    mesh.material.map?.dispose();
    mesh.material.dispose();
    for (const edge of mesh.children) {
      edge.geometry.dispose();
      edge.material.dispose();
    }
}

function disposePart(group) {
  for (const mesh of group.children) disposeMesh(mesh);
  group.clear();
}

function finishTexture(finish) {
  if (!finish || finish === 'smooth') return null;
  const size = 32, pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const noise = ((x * 73856093 ^ y * 19349663) >>> 0) % 31;
    const stripe = Math.sin(y * Math.PI / 2) * 7;
    const value = finish === 'brushed' ? 231 + stripe + noise / 4
      : finish === 'grained' ? 223 + noise : 239 + noise / 3;
    const offset = (y * size + x) * 4;
    pixels[offset] = pixels[offset + 1] = pixels[offset + 2] = value;
    pixels[offset + 3] = 255;
  }
  const texture = new DataTexture(pixels, size, size, RGBAFormat);
  texture.wrapS = texture.wrapT = RepeatWrapping;
  texture.magFilter = texture.minFilter = LinearFilter;
  texture.needsUpdate = true;
  return texture;
}

export function orbitPosition(center, radius, yaw, pitch, upAxis = 'z') {
  const angle = yaw * Math.PI / 180;
  if (upAxis === 'y') {
    const horizontal = Math.cos(pitch) * radius * 3;
    return new Vector3(center.x + Math.cos(angle) * horizontal,
      center.y + Math.sin(pitch) * radius * 3, center.z + Math.sin(angle) * horizontal);
  }
  return new Vector3(center.x + Math.cos(angle) * radius * 3,
    center.y + Math.sin(angle) * radius * 3, center.z + Math.sin(pitch) * radius * 2.1);
}

export function figureReviewViews(upAxis, endStage, startPose, endPose, middlePose = null) {
  const staged = endStage != null && endStage > 0 && startPose;
  return staged
      ? [[36, .48, 'Start · perspective', startPose, 0], [90, 0, 'Start · front', startPose, 0],
        [0, 0, 'Start · side', startPose, 0], [36, .48, 'Middle · perspective', middlePose, endStage / 2],
        [90, 0, 'Middle · front', middlePose, endStage / 2], [0, 0, 'Middle · side', middlePose, endStage / 2],
        [36, .48, 'End · perspective', endPose, endStage],
        [90, 0, 'End · front', endPose, endStage], [0, 0, 'End · side', endPose, endStage]]
      : upAxis === 'y'
        ? [[36, .48, 'Perspective', endPose, endStage], [90, 0, '+Z', endPose, endStage],
          [0, 0, '+X', endPose, endStage], [36, 1.45, 'Top', endPose, endStage]]
        : [[36, .48, 'Perspective', endPose, endStage], [0, 0, 'Front', endPose, endStage],
          [90, 0, 'Side', endPose, endStage], [36, 1.45, 'Top', endPose, endStage]];
}

export function createSceneViewer(canvas, upAxis = 'z') {
  const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: false, powerPreference: 'low-power' });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO));
  const scene = new Scene();
  const group = new Group();
  scene.add(group);
  scene.add(new AmbientLight(0xffffff, 1.65));
  const light = new DirectionalLight(0xffffff, 2.1);
  light.position.set(-3, upAxis === 'y' ? 7 : -4, upAxis === 'y' ? 4 : 7);
  scene.add(light);
  const camera = new OrthographicCamera(-1, 1, 1, -1, .01, 100);
  const annotations = createFigureAnnotations(scene, camera);
  camera.up.set(0, upAxis === 'y' ? 1 : 0, upAxis === 'y' ? 0 : 1);
  const raycaster = new Raycaster();
  const pointer = new Vector2();
  let currentParts = null;
  let currentYaw = 36;
  let currentSelectedId = '';
  let currentPitch = .48;
  let currentPose = null;
  let nativeOverlay = null;
  let currentMotionBounds = null, currentTimeline = null;
  let currentLabels = [], currentShowLabels = true, currentZoom = 1;
  let renderWidth = 0, renderHeight = 0;
  let selectedColor = new Color('#8caaff');
  let baseColor = new Color('#989aa3');

  function setParts(parts) {
    if (parts === currentParts) return;
    currentParts = parts;
    const existing = new Map(group.children.map(mesh => [mesh.userData.partId, mesh]));
    for (const part of parts) {
      if (!part.positions) continue;
      const previous = existing.get(part.id);
      if (previous?.userData.part === part) {
        existing.delete(part.id);
        continue;
      }
      const geometry = geometryForPart(part);
      const finish = {
        paint: { metalness: .12, roughness: .42 },
        metal: { metalness: .72, roughness: .3 },
        rubber: { metalness: 0, roughness: .86 },
        glass: { metalness: .15, roughness: .22 },
      }[part.material] || { metalness: .16, roughness: .67 };
      const material = new MeshStandardMaterial({ color: part.color || baseColor, ...finish,
        map: finishTexture(part.finish), side: DoubleSide,
        transparent: part.material === 'glass', opacity: part.material === 'glass' ? .38 : 1,
        depthWrite: part.material !== 'glass' });
      if (part.finish === 'matte') material.roughness = .93;
      if (part.finish === 'brushed') material.roughness = .38;
      const mesh = new Mesh(geometry, material);
      mesh.userData.partId = part.id;
      mesh.userData.part = part;
      mesh.userData.edge = part.edge;
      mesh.userData.partColor = part.color ? new Color(part.color) : null;
      if (part.edge === 'crisp' || part.edge === 'soft') {
        const edges = new LineSegments(new EdgesGeometry(geometry, part.edge === 'crisp' ? 16 : 48),
          new LineBasicMaterial({ color: baseColor, transparent: true, opacity: part.edge === 'crisp' ? .57 : .2 }));
        mesh.add(edges);
      }
      group.add(mesh);
    }
    for (const mesh of existing.values()) {
      group.remove(mesh);
      disposeMesh(mesh);
    }
  }

  function setTheme() {
    const root = canvas.closest('.island') || canvas;
    const styles = getComputedStyle(root);
    const text = cssRgb(styles.getPropertyValue('--text'), [238, 240, 245]);
    const accent = cssRgb(styles.getPropertyValue('--accent'), [140, 170, 255]);
    const background = cssRgb(styles.getPropertyValue('--bg'), BACKGROUND);
    baseColor = colorFromRgb(text.map((value, index) => Math.round(value * .59 + background[index] * .41)));
    selectedColor = colorFromRgb(accent);
    renderer.setClearColor(colorFromRgb(background), 1);
  }

  function setCamera(yaw, pitch, width, height, focusId = '', motionBounds = null, zoom = 1) {
    const focus = focusId && group.children.find(mesh => mesh.userData.partId === focusId);
    const bounds = focus ? new Box3().setFromObject(focus) : motionBounds || new Box3().setFromObject(group);
    const center = bounds.isEmpty() ? new Vector3() : bounds.getCenter(new Vector3());
    const radius = bounds.isEmpty() ? 2 : Math.max(.5, bounds.getSize(new Vector3()).length() / 2);
    camera.position.copy(orbitPosition(center, radius, yaw, pitch, upAxis));
    const distance = camera.position.distanceTo(center);
    camera.near = Math.max(.001, distance - radius * 1.05);
    camera.far = distance + radius * 1.05;
    camera.lookAt(center);
    camera.zoom = zoom;
    camera.updateMatrixWorld();
    const aspect = Math.max(.1, width / Math.max(1, height));
    if (!focus && motionBounds && !motionBounds.isEmpty()) {
      const halfHeight = radius * 1.1 / Math.min(1, aspect);
      camera.left = -halfHeight * aspect;
      camera.right = halfHeight * aspect;
      camera.top = halfHeight;
      camera.bottom = -halfHeight;
      camera.updateProjectionMatrix();
      return;
    }
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    group.traverse(object => {
      if (!object.isMesh || (focus && object !== focus)) return;
      const box = object.geometry.boundingBox;
      for (const x of [box.min.x, box.max.x]) for (const y of [box.min.y, box.max.y]) for (const z of [box.min.z, box.max.z]) {
        const point = new Vector3(x, y, z).applyMatrix4(object.matrixWorld).applyMatrix4(camera.matrixWorldInverse);
        minX = Math.min(minX, point.x); maxX = Math.max(maxX, point.x);
        minY = Math.min(minY, point.y); maxY = Math.max(maxY, point.y);
      }
    });
    const extentX = Number.isFinite(minX) ? maxX - minX : 1;
    const extentY = Number.isFinite(minY) ? maxY - minY : 1;
    const halfHeight = Math.max(.45, extentY * .62, extentX * .62 / aspect);
    camera.left = -halfHeight * aspect;
    camera.right = halfHeight * aspect;
    camera.top = halfHeight;
    camera.bottom = -halfHeight;
    camera.updateProjectionMatrix();
  }

  function draw(parts, yaw, selectedId, pitch = .48, size = null, focusSelected = false, pose = null, motionBounds = null, timeline = null, publishNative = true, labels = currentLabels, showLabels = currentShowLabels, zoom = currentZoom) {
    setParts(parts);
    setTheme();
    currentYaw = yaw;
    currentSelectedId = selectedId;
    currentPitch = pitch;
    currentPose = pose;
    currentMotionBounds = motionBounds;
    currentTimeline = timeline;
    currentLabels = labels ?? [];
    currentShowLabels = showLabels;
    currentZoom = zoom;
    const rect = canvas.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return;
    const width = size?.[0] ?? Math.round(rect.width);
    const height = size?.[1] ?? Math.round(rect.height);
    if (width !== renderWidth || height !== renderHeight) {
      renderer.setSize(width, height, false);
      renderWidth = width;
      renderHeight = height;
    }
    const visibility = timeline ? figureVisibility(parts, timeline.stage, timeline.elapsed) : null;
    for (const mesh of group.children) {
      mesh.visible = visibility?.get(mesh.userData.partId) !== false;
      mesh.matrixAutoUpdate = false;
      const matrix = pose?.get(mesh.userData.partId);
      if (matrix) mesh.matrix.copy(matrix);
      else mesh.matrix.identity();
      mesh.matrixWorldNeedsUpdate = true;
      const selected = mesh.userData.partId === selectedId;
      mesh.material.color.copy(mesh.userData.partColor || baseColor);
      mesh.material.emissive.copy(selected ? selectedColor : baseColor).multiplyScalar(selected ? .16 : .01);
      const motion = mesh.userData.part.motion;
      if (timeline && (motion?.tick?.tint || motion?.tick?.glow ||
        motion?.keys.some(key => key.tint != null || key.glow != null))) {
        const base = mesh.material.color.clone().convertLinearToSRGB().toArray();
        const { tint, glow } = figureAppearance(mesh.userData.part, timeline.stage, base, timeline.elapsed);
        mesh.material.color.setRGB(...tint, SRGBColorSpace);
        mesh.material.emissive.add(mesh.material.color.clone().multiplyScalar(glow));
      }
      if (mesh.children[0]) {
        mesh.children[0].material.color.copy(selected ? selectedColor : baseColor);
        mesh.children[0].material.opacity = selected ? .8 : mesh.userData.edge === 'crisp' ? .57 : .2;
      }
    }
    group.updateMatrixWorld(true);
    setCamera(yaw, pitch, width, height, focusSelected ? selectedId : '', motionBounds, zoom);
    annotations.draw(showLabels ? figureAnnotations(currentLabels, parts, selectedId) : [], pose, visibility, height);
    if (!size && nativeOverlay?.isPresenting()) renderer.clear();
    else renderer.render(scene, camera);
    if (!size && (publishNative || !nativeOverlay?.isPresenting())) nativeOverlay?.draw(group.children, camera, timeline, selectedId, currentLabels, showLabels);
  }

  function pick(x, y) {
    const rect = canvas.getBoundingClientRect();
    pointer.set(x / rect.width * 2 - 1, -(y / rect.height * 2 - 1));
    raycaster.setFromCamera(pointer, camera);
    return raycaster.intersectObjects(group.children.filter(mesh => mesh.visible), false)[0]?.object.userData.partId || null;
  }

  function preview(parts, focusSelected = false, displayedParts = parts, previewPose = null, previewStage = null, startPose = null, stageSeconds = 0, reviewBounds = null, reviewStart = 0) {
    const output = document.createElement('canvas');
    const staged = previewStage != null && previewStage > 0 && startPose;
    const width = staged ? 220 : 300, height = staged ? 145 : 210;
    output.width = width * (staged ? 3 : 2);
    output.height = height * (staged ? 3 : 2);
    const context = output.getContext('2d');
    if (!context) return null;
    const middlePose = staged ? figurePose(parts, previewStage / 2, reviewStart + previewStage * stageSeconds / 2) : null;
    const views = figureReviewViews(upAxis, previewStage, startPose, previewPose, middlePose);
    const originalYaw = currentYaw, originalPitch = currentPitch, originalSelection = currentSelectedId;
    const originalPose = currentPose;
    const originalMotionBounds = currentMotionBounds;
    const originalTimeline = currentTimeline;
    const originalZoom = currentZoom;
    for (let index = 0; index < views.length; index++) {
      const [yaw, pitch, label, pose, stage] = views[index];
      draw(parts, yaw, '', pitch, [width, height], false, pose, staged ? reviewBounds : null,
        stage == null ? null : { stage, elapsed: reviewStart + stage * stageSeconds }, true, currentLabels, currentShowLabels, 1);
      const x = (index % (staged ? 3 : 2)) * width, y = Math.floor(index / (staged ? 3 : 2)) * height;
      context.drawImage(canvas, x, y, width, height);
      context.fillStyle = '#f4f5f7';
      context.font = '13px sans-serif';
      context.fillText(label, x + 14, y + 24);
    }
    draw(displayedParts, originalYaw, originalSelection, originalPitch, null, focusSelected, originalPose, originalMotionBounds, originalTimeline, true, currentLabels, currentShowLabels, originalZoom);
    let jpeg = output.toDataURL('image/jpeg', .68).split(',')[1];
    if (jpeg.length > 180_000) jpeg = output.toDataURL('image/jpeg', .48).split(',')[1];
    return jpeg.length <= 180_000 ? jpeg : null;
  }

  return {
    draw, pick, preview,
    isNativePresenting: () => Boolean(nativeOverlay?.isPresenting()),
    setNative(enabled, onFailure, onChange) {
      if (enabled && !nativeOverlay) nativeOverlay = createNativeFigureOverlay(canvas, onFailure, onChange);
      if (!enabled && nativeOverlay) {
        nativeOverlay.dispose();
        nativeOverlay = null;
      }
    },
    dispose() {
      nativeOverlay?.dispose();
      annotations.dispose();
      disposePart(group);
      renderer.dispose();
      renderer.forceContextLoss();
    },
  };
}
