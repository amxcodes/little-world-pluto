import { Box3, Euler, Matrix4, Quaternion, Vector3 } from 'three';

const degrees = Math.PI / 180;
const axes = { x: new Vector3(1, 0, 0), y: new Vector3(0, 1, 0), z: new Vector3(0, 0, 1) };
const keyTime = key => key.at ?? key.step;
const rest = { step: 0, translate: [0, 0, 0], rotate: [0, 0, 0] };

function poseTime(motion, stage, elapsed) {
  const cycle = motion?.cycle;
  if (!cycle) return stage;
  const phase = ((elapsed % cycle.period) / cycle.period + (cycle.phase ?? 0)) % 1;
  return cycle.ping_pong ? 1 - Math.abs(1 - 2 * phase) : phase;
}

function quaternion(angles) {
  return new Quaternion().setFromEuler(new Euler(...angles.map(angle => angle * degrees), 'ZYX'));
}

export function channelPose(channels, time) {
  const values = [0, 0, 0, 0, 0, 0], seen = new Set();
  const names = ['translate.x', 'translate.y', 'translate.z', 'rotate.x', 'rotate.y', 'rotate.z'];
  for (const track of channels ?? []) {
    const i = names.indexOf(track.channel);
    if (i < 0) continue;
    if (!seen.has(i) || time >= track.start) {
      const t = Math.min(1, Math.max(0, (time - track.start) / (track.end - track.start)));
      values[i] = track.from + (track.to - track.from) * t * t * (3 - 2 * t);
    }
    seen.add(i);
  }
  return { translate: values.slice(0, 3), rotate: values.slice(3) };
}

export function hasPeriodicMotion(part) {
  return Boolean(part.motion?.spin || part.motion?.bob || part.motion?.pulse || part.motion?.orbit || part.motion?.path || part.motion?.tick || part.motion?.cycle);
}

export function figureReviewDuration(parts, last, stageSeconds) {
  const finite = parts.some(part => !part.motion?.cycle && (part.motion?.keys?.length || part.motion?.channels?.length));
  if (finite && last > 0) return last * stageSeconds;
  let duration = 0;
  for (const {motion} of parts) {
    for (const track of [motion?.cycle, motion?.tick, motion?.path, motion?.spin, motion?.bob, motion?.pulse, motion?.orbit]) {
      duration = Math.max(duration, track?.period ?? 0);
    }
  }
  return duration || last * stageSeconds;
}

export function motionActiveAt(part, elapsed) {
  const motion = part.motion;
  return Boolean(motion?.spin || motion?.bob || motion?.pulse || motion?.orbit || motion?.tick || motion?.cycle ||
    (motion?.path && (!motion.path.once || elapsed < motion.path.period)));
}

function pathOffset({ points, period, once }, elapsed) {
  const segments = once ? points.length - 1 : points.length;
  const t = once ? Math.min(1, Math.max(0, elapsed / period)) : (elapsed % period) / period;
  const phase = (once ? t * t * (3 - 2 * t) : t) * segments;
  const index = Math.min(segments - 1, Math.floor(phase));
  const f = phase - index, u = 1 - f;
  const point = offset => points[once
    ? Math.min(points.length - 1, Math.max(0, index + offset))
    : (index + offset + points.length) % points.length];
  const [a, b, c, d] = [-1, 0, 1, 2].map(point);
  return b.map((value, axis) => {
    const b1 = value + (c[axis] - a[axis]) / 6;
    const b2 = c[axis] - (d[axis] - value) / 6;
    return u ** 3 * value + 3 * u * u * f * b1 + 3 * u * f * f * b2 + f ** 3 * c[axis];
  });
}

function appearanceTrack(keys, start, time, read, fallback) {
  let before = fallback;
  for (const key of keys) {
    const next = read(key);
    if (next == null) continue;
    if (keyTime(key) > time) {
      const t = Math.min(1, Math.max(0, (time - start) / Math.max(Number.EPSILON, keyTime(key) - start)));
      const amount = t * t * (3 - 2 * t);
      return before.map((value, index) => value + (next[index] - value) * amount);
    }
    start = keyTime(key);
    before = next;
  }
  return before;
}

const tickAmount = (tick, elapsed) =>
  (1 - Math.cos(2 * Math.PI * ((elapsed % tick.period) / tick.period + (tick.phase ?? 0)))) * .5;

export function figureAppearance(part, time, base, elapsed = 0) {
  const keys = part.motion?.keys ?? [];
  time = poseTime(part.motion, time, elapsed);
  let tint = appearanceTrack(keys, part.step, time, key => key.tint, base);
  let glow = appearanceTrack(keys, part.step, time, key => key.glow == null ? null : [key.glow], [0])[0];
  const tick = part.motion?.tick;
  if (tick) {
    const amount = tickAmount(tick, elapsed);
    if (tick.tint) tint = tick.tint[0].map((value, i) => value + (tick.tint[1][i] - value) * amount);
    if (tick.glow) glow = tick.glow[0] + (tick.glow[1] - tick.glow[0]) * amount;
  }
  return { tint, glow };
}

export function figureVisibility(parts, time, elapsed = 0) {
  const visibility = new Map();
  for (const part of parts) {
    let shown = true;
    const sample = poseTime(part.motion, time, elapsed);
    for (const key of part.motion?.keys ?? []) {
      if (keyTime(key) > sample) break;
      if (key.show != null) shown = key.show;
    }
    const tick = part.motion?.tick;
    if (tick?.show) shown = tick.show[Number(tickAmount(tick, elapsed) >= .5)];
    visibility.set(part.id, (part.step ?? 0) <= time && shown && visibility.get(part.parent) !== false);
  }
  return visibility;
}

export function figureBindMatrix(part) {
  const t = part.transform ?? {};
  return new Matrix4().compose(new Vector3(...(t.position ?? [0,0,0])),
    quaternion(t.rotation ?? [0,0,0]), new Vector3(...(t.scale ?? [1,1,1])));
}

export function figurePose(parts, time, elapsed = 0) {
  const matrices = new Map(), frames = new Map();
  for (const part of parts) {
    const parent = (part.parent_local ? frames : matrices).get(part.parent);
    if (!part.motion) {
      if (parent) matrices.set(part.id, parent);
      frames.set(part.id, (parent?.clone() ?? new Matrix4()).multiply(figureBindMatrix(part)));
      continue;
    }
    const sample = poseTime(part.motion, time, elapsed);
    let before = { ...rest, step: part.step };
    let after = before;
    for (const key of part.motion.keys) {
      after = key;
      if (keyTime(key) > sample) break;
      before = key;
    }
    const fraction = keyTime(after) > keyTime(before)
      ? Math.min(1, Math.max(0, (sample - keyTime(before)) / (keyTime(after) - keyTime(before)))) : 0;
    const amount = fraction * fraction * (3 - 2 * fraction);
    const position = new Vector3(...before.translate).lerp(new Vector3(...after.translate), amount);
    const rotation = quaternion(before.rotate).slerp(quaternion(after.rotate), amount);
    if (part.motion.channels?.length) {
      const channels = channelPose(part.motion.channels, sample);
      position.fromArray(channels.translate);
      rotation.copy(quaternion(channels.rotate));
    }
    const spin = part.motion.spin;
    if (spin) {
      rotation.multiply(new Quaternion().setFromAxisAngle(axes[spin.axis],
        2 * Math.PI * ((elapsed % spin.period) / spin.period)));
    }
    const pivot = new Vector3(...part.motion.pivot);
    // Geometry and hinges share bind-space coordinates. Apply local motion
    // first, then the ancestor motion, so child hinges follow the assembly.
    const wave = period => Math.sin(2 * Math.PI * ((elapsed % period) / period));
    const { bob, pulse } = part.motion;
    const beforeScale = before.scale ?? 1, afterScale = after.scale ?? 1;
    const stagedScale = beforeScale + amount * (afterScale - beforeScale);
    let scale = stagedScale * (pulse ? 1 + pulse.amplitude * wave(pulse.period) : 1);
    if (bob) position.addScaledVector(new Vector3(...bob.offset), wave(bob.period));
    const orbit = part.motion.orbit;
    if (orbit) {
      const radial = pivot.clone().sub(new Vector3(...orbit.center));
      const angle = 2 * Math.PI * ((elapsed % orbit.period) / orbit.period);
      position.add(radial.clone().applyAxisAngle(axes[orbit.axis], angle).sub(radial));
    }
    if (part.motion.path) position.add(new Vector3(...pathOffset(part.motion.path, elapsed)));
    const tick = part.motion.tick;
    if (tick) {
      const amount = tickAmount(tick, elapsed);
      if (tick.translate) position.fromArray(tick.translate[0]).lerp(new Vector3(...tick.translate[1]), amount);
      if (tick.rotate) rotation.copy(quaternion(tick.rotate[0])).slerp(quaternion(tick.rotate[1]), amount);
      if (tick.scale) scale = tick.scale[0] + (tick.scale[1] - tick.scale[0]) * amount;
    }
    position.add(pivot).sub(pivot.clone().multiplyScalar(scale).applyQuaternion(rotation));
    const matrix = new Matrix4().compose(position, rotation, new Vector3().setScalar(scale));
    if (parent) matrix.premultiply(parent);
    matrices.set(part.id, matrix);
    frames.set(part.id, matrix.clone().multiply(figureBindMatrix(part)));
  }
  return matrices;
}

// Bound each local transform over all phases, then walk from the mesh toward
// the root. This also covers unrelated periods without a long sampling window.
function sweepLocal(box, motion) {
  if (!motion) return;
  const { pivot, spin, bob, pulse, orbit, tick } = motion;
  const scales = [1, ...motion.keys.map(key => key.scale ?? 1)];
  const lowScale = tick?.scale ? Math.min(...tick.scale) : Math.min(...scales) * (1 - (pulse?.amplitude ?? 0));
  const highScale = tick?.scale ? Math.max(...tick.scale) : Math.max(...scales) * (1 + (pulse?.amplitude ?? 0));
  const min = box.min.toArray().map((value, axis) => value - pivot[axis]);
  const max = box.max.toArray().map((value, axis) => value - pivot[axis]);
  const reach = min.map((value, axis) => Math.max(Math.abs(value), Math.abs(max[axis])));
  const rotates = motion.channels?.some(track => track.channel.startsWith("rotate.") && (track.from !== 0 || track.to !== 0)) || Boolean(tick?.rotate) || motion.keys.some(key => key.rotate.some(angle => angle !== 0));
  const spinAxis = spin ? ['x', 'y', 'z'].indexOf(spin.axis) : -1;
  const orbitAxis = orbit ? ['x', 'y', 'z'].indexOf(orbit.axis) : -1;
  const radial = orbit ? pivot.map((value, axis) => value - orbit.center[axis]) : [0, 0, 0];
  const orbitReach = Math.hypot(...radial.filter((_, axis) => axis !== orbitAxis));
  for (let axis = 0; axis < 3; axis++) {
    const orbits = orbit && axis !== orbitAxis;
    const orbitLow = orbits ? -orbitReach - radial[axis] : 0;
    const orbitHigh = orbits ? orbitReach - radial[axis] : 0;
    const translations = tick?.translate?.map(value => value[axis]) ?? [0, ...motion.keys.map(key => key.translate[axis])];
    for (const track of motion.channels ?? []) {
      if (track.channel === `translate.${"xyz"[axis]}`) translations.push(track.from, track.to);
    }
    const path = motion.path?.points.map(point => point[axis]) ?? [0];
    const pathMin = Math.min(...path), pathMax = Math.max(...path);
    const overshoot = (pathMax - pathMin) / 6;
    const bobReach = Math.abs(bob?.offset[axis] ?? 0);
    let low = Math.min(min[axis] * lowScale, min[axis] * highScale);
    let high = Math.max(max[axis] * lowScale, max[axis] * highScale);
    if (rotates || (spin && axis !== spinAxis)) {
      high = highScale * Math.hypot(...reach.filter((_, index) => rotates || index !== spinAxis));
      low = -high;
    }
    box.min.setComponent(axis, pivot[axis] + low + Math.min(...translations) - bobReach + orbitLow + pathMin - overshoot);
    box.max.setComponent(axis, pivot[axis] + high + Math.max(...translations) + bobReach + orbitHigh + pathMax + overshoot);
  }
}

export function figureMotionBounds(parts, last) {
  const envelope = new Box3();
  const boxes = parts.map(part => {
    if (!part.bounds) return null;
    const center = new Vector3(...part.bounds.center);
    const half = new Vector3(...part.bounds.size).multiplyScalar(.5);
    return new Box3(center.clone().sub(half), center.clone().add(half));
  });
  for (let tick = 0; tick <= last * 8; tick++) {
    const time = tick / 8;
    const matrices = figurePose(parts, time);
    for (const [index, part] of parts.entries()) {
      if (part.step > time || !boxes[index]) continue;
      const matrix = matrices.get(part.id);
      envelope.union(matrix ? boxes[index].clone().applyMatrix4(matrix) : boxes[index]);
    }
  }
  const needsSweep = part => part.motion?.channels?.length || hasPeriodicMotion(part) || part.motion?.keys.some(key => key.scale != null);
  if (parts.some(needsSweep)) {
    const byId = new Map(parts.map(part => [part.id, part]));
    for (const [index, part] of parts.entries()) {
      if (!boxes[index]) continue;
      const chain = [];
      for (let node = part; node; node = byId.get(node.parent)) chain.push(node);
      if (!chain.some(needsSweep)) continue;
      const swept = boxes[index].clone();
      for (let i = 0; i < chain.length; i++) {
        if (i > 0 && chain[i - 1].parent_local) swept.applyMatrix4(figureBindMatrix(chain[i]));
        sweepLocal(swept, chain[i].motion);
      }
      envelope.union(swept);
    }
  }
  return envelope;
}



function worldBounds(part, pose, basis) {
  const center = new Vector3(...part.bounds.center);
  const half = new Vector3(...part.bounds.size).multiplyScalar(.5);
  const box = new Box3(center.clone().sub(half), center.add(half));
  const motion = pose.get(part.id);
  if (basis) box.applyMatrix4(motion ? basis.clone().multiply(motion) : basis);
  else if (motion) box.applyMatrix4(motion);
  return box;
}

function boundsGap(a, b) {
  const gap = axis => Math.max(0, a.min[axis] - b.max[axis], b.min[axis] - a.max[axis]);
  return Math.hypot(gap('x'), gap('y'), gap('z'));
}

// A separated AABB proves a gap. Overlapping AABBs do not prove contact.
export function figureAssemblyGaps(parts, last, duration = 0, start = 0) {
  if (last < 0) return [];
  const separated = new Map();
  for (let sample = 0; sample <= (last > 0 ? 16 : 0); sample++) {
    const stage = last * sample / 16;
    const elapsed = start + duration * sample / 16;
    const pose = figurePose(parts, stage, elapsed);
    const visible = figureVisibility(parts, stage, elapsed);
    const meshes = parts.filter(part => part.bounds && part.shape?.kind !== 'arrow' && visible.get(part.id) !== false);
    if (meshes.length < 2) continue;
    const boxes = meshes.map(part => worldBounds(part, pose));
    const envelope = boxes.reduce((box, next) => box.union(next), new Box3());
    const size = envelope.getSize(new Vector3());
    const tolerance = .025 * Math.max(size.x, size.y, size.z);
    const pending = new Set(meshes.keys());
    const groups = [];
    while (pending.size) {
      const first = pending.values().next().value;
      pending.delete(first);
      const group = [first];
      for (const member of group) {
        for (const index of pending) {
          if (boundsGap(boxes[member], boxes[index]) <= tolerance) {
            pending.delete(index);
            group.push(index);
          }
        }
      }
      groups.push(group);
    }
    // The largest component is the reference assembly.
    groups.sort((a, b) => b.length - a.length);
    for (const group of groups.slice(1)) {
      const ids = group.map(i => meshes[i].id).sort();
      let gap = Infinity;
      for (const a of group) for (const b of groups[0]) gap = Math.min(gap, boundsGap(boxes[a], boxes[b]));
      const key = ids.join(',');
      if (gap > (separated.get(key)?.gap ?? 0)) separated.set(key, { ids, gap, stage });
    }
  }
  return [...separated.values()].sort((a, b) => b.gap - a.gap).slice(0, 6);
}

// Endpoints are measured in the common root's frame so whole-model motion
// cannot hide a backwards fold or a misaligned assembly.
function figureEndpointDefects(parts, last, plan, duration, start) {
  if (!plan) return [];
  const byId = new Map(parts.map(part => [part.id, part]));
  const format = values => values.map(value => value.toFixed(2)).join(',');
  const defects = [];
  const starts = new Map();
  for (const [stage, endpoint, elapsed] of [[0, 'start', start], [last, 'end', start + duration]]) {
    const pose = figurePose(parts, stage, elapsed);
    const frame = part => (pose.get(part.id)?.clone() ?? new Matrix4()).multiply(figureBindMatrix(part));
    for (const role of plan.assemblies) {
      if (!role.landmarks?.length && !(endpoint === 'end' && role.end_box)) continue;
      const part = byId.get(role.id);
      if (!part) continue;
      let root = part;
      while (root.parent && byId.has(root.parent)) root = byId.get(root.parent);
      const rootInverse = frame(root).invert();
      if (endpoint === 'end' && role.end_box) {
        const descendants = new Set([part.id]);
        const actual = new Box3();
        for (const child of parts) {
          if (descendants.has(child.parent)) descendants.add(child.id);
          if (descendants.has(child.id) && child.bounds && child.shape?.kind !== 'arrow') {
            actual.union(worldBounds(child, pose, rootInverse));
          }
        }
        const allowed = new Box3(new Vector3(...role.end_box.min), new Vector3(...role.end_box.max));
        if (!actual.isEmpty() && !allowed.clone().expandByScalar(.05).containsBox(actual)) {
          defects.push(`FIT FAILURE ${part.id} end: assembled geometry reaches [${format(actual.min.toArray())}] to [${format(actual.max.toArray())}], outside the available space [${format(allowed.min.toArray())}] to [${format(allowed.max.toArray())}] in ${root.id}. Fold or reposition its actual geometry to fit.`);
        }
      }
      const relative = rootInverse.multiply(frame(part));
      for (const [index, mark] of (role.landmarks ?? []).entries()) {
        const key = `${role.id}:${index}`;
        const actual = new Vector3(...mark.point).applyMatrix4(relative);
        if (endpoint === 'start') starts.set(key, actual.clone());
        const opening = starts.get(key);
        const intendedTravel = new Vector3(...mark.end).distanceTo(new Vector3(...mark.start));
        const error = actual.distanceTo(new Vector3(...mark[endpoint]));
        if (error > mark.tolerance && endpoint === 'end' && opening
          && intendedTravel > 2 * mark.tolerance && opening.distanceTo(actual) <= mark.tolerance) {
          defects.push(`ENDPOINT FAILURE ${part.id} end: landmark [${format(mark.point)}] stays at [${format(actual.toArray())}] but must travel ${intendedTravel.toFixed(2)} scene units. Put the landmark on the group that physically carries it and give that group a reachable move.`);
          continue;
        }
        if (error > mark.tolerance) {
          defects.push(`ENDPOINT FAILURE ${part.id} ${endpoint}: local point [${format(mark.point)}] reaches [${format(actual.toArray())}], expected [${format(mark[endpoint])}] in ${root.id}; error ${error.toFixed(2)}.`);
        }
      }
    }
  }
  return defects;
}

export function figurePoseInspection(parts, last, plan, duration = 0, start = 0) {
  const format = values => values.map(value => value.toFixed(2)).join(',');
  const jointIssues = figureJointDefects(parts, last, duration, start).map(issue => `JOINT FAILURE ${issue.part} -> ${issue.parent}: gap ${issue.gap.toFixed(3)} at pose ${issue.stage.toFixed(2)}. Repair hierarchy, anchors or motion to preserve declared contact.`);
  const gaps = figureAssemblyGaps(parts, last, duration, start).map(({ids, gap, stage}) =>
    `ASSEMBLY GAP ${ids.slice(0, 6).join(', ')}: separated from the main assembly by at least ${gap.toFixed(2)} at pose ${stage.toFixed(2)}. Connect intended load-bearing parts and inspect clearance throughout the move.`);
  const endpoints = figureEndpointDefects(parts, last, plan, duration, start);
  const branches = new Map();
  for (const part of parts) {
    if (!part.bounds || part.shape?.kind === 'arrow') continue;
    const owner = part.parent ?? 'root';
    if (!branches.has(owner)) branches.set(owner, []);
    branches.get(owner).push(`${part.id} (${part.shape?.kind ?? 'mesh'})`);
  }
  const detail = [...branches].map(([owner, features]) => `${owner}: ${features.join(', ')}`).join('; ');
  return [...endpoints, ...jointIssues, ...gaps, `AUTHORED GEOMETRY BY SUPPORTING GROUP: ${detail}`].join('\n')
    + '\n' + [0, last / 2, last].map((stage, index) => {
    const elapsed = start + duration * index / 2;
    const pose = figurePose(parts, stage, elapsed);
    const visible = figureVisibility(parts, stage, elapsed);
    const measured = parts.filter(part => part.bounds && visible.get(part.id) !== false).slice(0, 36).map(part => {
      const box = worldBounds(part, pose);
      return `${part.id}: center ${format(box.getCenter(new Vector3()).toArray())}, span ${format(box.getSize(new Vector3()).toArray())}`;
    });
    return `${['START','MIDDLE','END'][index]} world-space axis-aligned bounds (not proof of contact): ${measured.join('; ')}`;
  }).join('\n');
}


// Anchors are expressed in each part's own shape coordinates before its bind
// transform. Measure, never silently snap: a bad joint needs an authored repair.
export function figureJointDefects(parts, last, duration = 0, start = 0) {
  const defects = new Map();
  for (let sample = 0; sample <= 16; sample++) {
    const stage = last * sample / 16;
    const pose = figurePose(parts, stage, start + duration * sample / 16);
    const frames = new Map(parts.map(part => [part.id,
      (pose.get(part.id)?.clone() ?? new Matrix4()).multiply(figureBindMatrix(part))]));
    for (const part of parts) {
      if (!part.joint || !frames.has(part.parent)) continue;
      const a = new Vector3(...part.joint.anchor).applyMatrix4(frames.get(part.id));
      const b = new Vector3(...part.joint.parent_anchor).applyMatrix4(frames.get(part.parent));
      const gap = a.distanceTo(b);
      if (gap > part.joint.tolerance && gap > (defects.get(part.id)?.gap ?? 0))
        defects.set(part.id, {part:part.id,parent:part.parent,gap,stage});
    }
  }
  return [...defects.values()];
}
