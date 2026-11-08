import {
  BoxGeometry, BufferGeometry, Float32BufferAttribute, CapsuleGeometry, CatmullRomCurve3, ConeGeometry, CylinderGeometry, Euler, ExtrudeGeometry,
  LatheGeometry, Matrix4, MeshStandardMaterial, Path, Quaternion, Shape, ShapeUtils, SphereGeometry, TorusGeometry,
  TubeGeometry, Vector2, Vector3, DodecahedronGeometry, IcosahedronGeometry,
  OctahedronGeometry, TetrahedronGeometry,
} from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';

import { MarchingCubes } from 'three/addons/objects/MarchingCubes.js';
import { mergeGeometries, mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';

const solids = {
  tetrahedron: TetrahedronGeometry, octahedron: OctahedronGeometry,
  dodecahedron: DodecahedronGeometry, icosahedron: IcosahedronGeometry,
};
const shapes = new Map();
const axes = { x: 0, y: 1, z: 2 };
const degrees = Math.PI / 180;

function extrude(shape, depth, bevel = 0) {
  bevel ??= 0;
  const coreDepth = depth - 2 * bevel;
  return new ExtrudeGeometry(shape, {
    depth: coreDepth, bevelEnabled: bevel > 0, bevelThickness: bevel,
    bevelSize: bevel, bevelSegments: 2, curveSegments: 16,
  }).translate(0, 0, -coreDepth / 2);
}

function roundedProfile(points, corners, radius) {
  const outline = points.map(point => new Vector2(...point));
  return outline.flatMap((point, index) => {
    if (!corners.includes(index)) return [point];
    const before = outline[(index + outline.length - 1) % outline.length].clone().sub(point);
    const after = outline[(index + 1) % outline.length].clone().sub(point);
    const trim = Math.min(radius, before.length() * .45, after.length() * .45);
    const start = point.clone().add(before.normalize().multiplyScalar(trim));
    const end = point.clone().add(after.normalize().multiplyScalar(trim));
    return Array.from({ length: 5 }, (_, sample) => {
      const t = sample / 4;
      return start.clone().multiplyScalar((1 - t) ** 2)
        .addScaledVector(point, 2 * t * (1 - t)).addScaledVector(end, t * t);
    });
  });
}

function buildBlob({ field, blend }) {
  const low = new Vector3(Infinity, Infinity, Infinity);
  const high = new Vector3(-Infinity, -Infinity, -Infinity);
  const elements = field.map(element => {
    const start = new Vector3(...(element.center ?? element.from));
    const end = element.to ? new Vector3(...element.to) : start;
    const axis = end.clone().sub(start);
    const padding = Math.max(element.radius, element.to_radius ?? element.radius) + blend;
    for (const point of [start, end]) {
      low.min(point.clone().addScalar(-padding));
      high.max(point.clone().addScalar(padding));
    }
    return { start, axis, lengthSquared: axis.lengthSq(), radius: element.radius,
      endRadius: element.to_radius ?? element.radius };
  });
  const center = low.clone().lerp(high, .5);
  const half = high.clone().sub(low).multiplyScalar(.60);
  const resolution = 36, maxTriangles = 12_000;
  const material = new MeshStandardMaterial();
  const volume = new MarchingCubes(resolution, material, false, false, maxTriangles);
  volume.isolation = 0;
  const sample = new Vector3(), relative = new Vector3();
  for (let z = 0, index = 0; z < resolution; z++) {
    for (let y = 0; y < resolution; y++) {
      for (let x = 0; x < resolution; x++, index++) {
        sample.set(2*x/resolution-1, 2*y/resolution-1, 2*z/resolution-1)
          .multiply(half).add(center);
        let distance = Infinity;
        for (const element of elements) {
          relative.subVectors(sample, element.start);
          let fraction = 0;
          if (element.lengthSquared > 0) {
            const t = relative.dot(element.axis) / element.lengthSquared;
            fraction = Math.min(1, Math.max(0, t));
            relative.addScaledVector(element.axis, -fraction);
          }
          const next = relative.length() - (element.radius + (element.endRadius - element.radius) * fraction);
          const weight = blend ? Math.max(blend - Math.abs(next - distance), 0) / blend : 0;
          distance = Math.min(next, distance) - weight * weight * blend / 4;
        }
        volume.field[index] = -distance;
      }
    }
  }
  volume.update();
  volume.geometry.dispose();
  material.dispose();
  if (!volume.count || volume.count / 3 > maxTriangles) throw Error('Blob exceeds its mesh budget or has no surface');
  const mesh = new BufferGeometry();
  mesh.setAttribute('position', new Float32BufferAttribute(volume.positionArray.slice(0, volume.count * 3), 3));
  mesh.setAttribute('normal', new Float32BufferAttribute(volume.normalArray.slice(0, volume.count * 3), 3));
  return mesh.scale(half.x, half.y, half.z).translate(center.x, center.y, center.z);
}

function buildOrganic({ radius, lumps = .12, seed = 1 }) {
  lumps ??= .12;
  seed ??= 1;
  const base = new IcosahedronGeometry(1, 4);
  base.deleteAttribute('normal');
  base.deleteAttribute('uv');
  const mesh = mergeVertices(base);
  base.dispose();
  const points = mesh.getAttribute('position');
  const phase = seed * 2.399963229728653;
  for (let i = 0; i < points.count; i++) {
    const x = points.getX(i), y = points.getY(i), z = points.getZ(i);
    const noise = (Math.sin(4*x + 2*y + phase) + Math.sin(3*y - 2*z + phase * .73)
      + Math.sin(5*z + x - phase * .41)) / 3;
    const size = radius * (1 + lumps * noise);
    points.setXYZ(i, x * size, y * size, z * size);
  }
  mesh.computeVertexNormals();
  return mesh;
}

function geometryFor(recipe) {
  const key = JSON.stringify(recipe);
  if (shapes.has(key)) return shapes.get(key);
  let mesh;
  switch (recipe.kind) {
    case 'box':
      mesh = recipe.bevel > 0
        ? new RoundedBoxGeometry(...recipe.size, 2, recipe.bevel)
        : new BoxGeometry(...recipe.size);
      break;
    case 'capsule': mesh = new CapsuleGeometry(recipe.radius, recipe.length, 8, 24); break;
    case 'poly': {
      const size = 2 * recipe.radius / Math.sqrt(3);
      mesh = recipe.solid === 'cube' ? new BoxGeometry(size, size, size)
        : new solids[recipe.solid](recipe.radius);
      break;
    }
    case 'sphere':
      mesh = new SphereGeometry(recipe.radius, recipe.segments ?? 20,
        recipe.segments == null ? 14 : Math.max(6, Math.round(recipe.segments * 2 / 3)), 0, 2 * Math.PI, 0,
        recipe.dome ? Math.PI / 2 : Math.PI);
      break;
    case 'arrow': {
      const points = recipe.path.map(point => new Vector3(...point));
      const path = new CatmullRomCurve3(points);
      const shaft = new TubeGeometry(path, Math.min(48, points.length * 4), recipe.radius, 8);
      const tail = new SphereGeometry(recipe.radius, 12, 8).translate(...points[0].toArray());
      const axis = path.getTangent(1).normalize();
      const head = new ConeGeometry(recipe.head_radius, recipe.head_length, 16);
      const rotation = new Quaternion().setFromUnitVectors(new Vector3(0, 1, 0), axis);
      const center = points.at(-1).clone().addScaledVector(axis, recipe.head_length / 2);
      head.applyQuaternion(rotation).translate(...center.toArray());
      const pieces = [shaft, tail, head];
      mesh = mergeGeometries(pieces);
      for (const piece of pieces) piece.dispose();
      if (!mesh) throw Error('Could not assemble the arrow geometry');
      break;
    }
    case 'cylinder': {
      const { radius, height, bevel } = recipe;
      const top = recipe.top_radius ?? radius;
      const arc = (recipe.arc ?? 360) * degrees;
      if (!bevel) {
        mesh = new CylinderGeometry(top, radius, height, recipe.segments ?? 24, 1, recipe.open === true, 0, arc);
        break;
      }
      const y = height / 2;
      const profile = top > 0
        ? [[0, -y], [radius, -y], [top, y], [0, y]]
        : [[0, -y], [radius, -y], [0, y]];
      mesh = new LatheGeometry(roundedProfile(profile, top > 0 ? [1, 2] : [1], bevel), recipe.segments ?? 24, 0, arc);
      break;
    }
    case 'torus': mesh = new TorusGeometry(recipe.radius, recipe.thickness, recipe.radial_segments ?? 10, recipe.segments ?? 28, (recipe.arc ?? 360) * degrees).rotateX(Math.PI / 2); break;
    case 'gear': {
      const points = Array.from({ length: recipe.teeth * 4 }, (_, index) => {
        const toothPhase = index % 4;
        const angle = index * 2 * Math.PI / (recipe.teeth * 4);
        const radius = toothPhase === 1 || toothPhase === 2 ? recipe.radius : recipe.root_radius;
        return new Vector2(Math.cos(angle) * radius, Math.sin(angle) * radius);
      });
      const outline = new Shape(points);
      if (recipe.bore) {
        const hole = new Path();
        hole.absarc(0, 0, recipe.bore, 0, 2 * Math.PI, true);
        outline.holes.push(hole);
      }
      mesh = extrude(outline, recipe.depth, recipe.bevel);
      break;
    }
    case 'spring': {
      const segments = recipe.turns * 16;
      const points = Array.from({ length: segments + 1 }, (_, index) => {
        const fraction = index / segments;
        const angle = fraction * recipe.turns * 2 * Math.PI;
        return new Vector3(recipe.radius * Math.cos(angle), recipe.height * (fraction - .5),
          recipe.radius * Math.sin(angle));
      });
      mesh = new TubeGeometry(new CatmullRomCurve3(points), segments, recipe.wire, 8);
      break;
    }
    case 'extrude': {
      const outline = new Shape(recipe.outline.map(point => new Vector2(...point)));
      for (const hole of recipe.holes ?? []) {
        outline.holes.push(new Path(hole.map(point => new Vector2(...point))));
      }
      mesh = extrude(outline, recipe.depth, recipe.bevel);
      break;
    }
    case 'sleeve': {
      const { radius, inner_radius: inner, height, bevel } = recipe;
      const y = height / 2;
      const profile = [[inner, -y], [radius, -y], [radius, y], [inner, y]];
      const points = bevel ? roundedProfile(profile, [0, 1, 2, 3], bevel) : profile.map(point => new Vector2(...point));
      mesh = new LatheGeometry([...points, points[0]], recipe.segments ?? 32, 0, (recipe.arc ?? 360) * degrees);
      break;
    }
    case 'blob': mesh = recipe.field?.length ? buildBlob(recipe) : buildOrganic(recipe); break;
    case 'surface': {
      const columns = recipe.grid[0].length;
      const indices = [];
      for (let row = 0; row < recipe.grid.length - 1; row++) {
        for (let column = 0; column < columns - 1; column++) {
          const a = row * columns + column;
          indices.push(a, a + columns, a + 1, a + 1, a + columns, a + columns + 1);
        }
      }
      mesh = new BufferGeometry();
      mesh.setAttribute('position', new Float32BufferAttribute(recipe.grid.flat(2), 3));
      mesh.setIndex(indices);
      mesh.computeVertexNormals();
      break;
    }
    case 'lathe': {
      const { profile, segments, arc, bevel } = recipe;
      const corners = profile.flatMap(([radius], index) =>
        index > 0 && index < profile.length - 1 && radius > 0 ? [index] : []);
      const points = bevel ? roundedProfile(profile, corners, bevel)
        : profile.map(point => new Vector2(...point));
      mesh = new LatheGeometry(points, segments ?? 24, 0, (arc ?? 360) * degrees);
      break;
    }
    case 'tube': {
      const path = new CatmullRomCurve3(recipe.path.map(point => new Vector3(...point)), recipe.closed === true);
      mesh = new TubeGeometry(path, recipe.segments ?? Math.min(48, recipe.path.length * 4), recipe.radius,
        recipe.radial_segments ?? 8, recipe.closed === true);
      break;
    }
    default: throw Error('Unsupported figure shape');
  }
  if (shapes.size >= 100) {
    const oldest = shapes.keys().next().value;
    shapes.get(oldest).dispose();
    shapes.delete(oldest);
  }
  shapes.set(key, mesh);
  return mesh;
}

function appendCutCap(segments, axis, cut, positions, normals) {
  const u = (axis + 1) % 3, v = (axis + 2) % 3;
  const vertices = new Map();
  const weld = point => {
    const x = point.getComponent(u), y = point.getComponent(v);
    const key = Math.round(x * 1e5) + ':' + Math.round(y * 1e5);
    if (!vertices.has(key)) vertices.set(key, { point: new Vector2(x, y), neighbors: new Set() });
    return vertices.get(key);
  };
  for (const [from, to] of segments) {
    const a = weld(from), b = weld(to);
    if (a !== b) {
      a.neighbors.add(b);
      b.neighbors.add(a);
    }
  }
  if ([...vertices.values()].some(node => node.neighbors.size !== 2)) {
    throw Error('Cut cap needs a closed manifold section');
  }
  const seen = new Set(), loops = [];
  for (const start of vertices.values()) {
    if (seen.has(start)) continue;
    let current = start, previous = null;
    const loop = [];
    do {
      if (seen.has(current)) throw Error('Cut cap has intersecting contours');
      seen.add(current);
      loop.push(current.point);
      const next = [...current.neighbors].find(node => node !== previous);
      previous = current;
      current = next;
    } while (current !== start);
    if (loop.length < 3) throw Error('Cut cap has a degenerate contour');
    loops.push({ loop, area: Math.abs(ShapeUtils.area(loop)), parent: null, depth: 0 });
  }
  const contains = (ring, p) => {
    let inside = false;
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
      const a = ring[i], b = ring[j];
      if ((a.y > p.y) !== (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y)) inside = !inside;
    }
    return inside;
  };
  loops.sort((a, b) => b.area - a.area);
  for (const [index, entry] of loops.entries()) {
    entry.parent = loops.slice(0, index).findLast(other => contains(other.loop, entry.loop[0])) ?? null;
    entry.depth = entry.parent ? entry.parent.depth + 1 : 0;
  }
  const sign = cut.keep_positive ? -1 : 1;
  const normal = [0, 0, 0];
  normal[axis] = sign;
  for (const outer of loops.filter(entry => entry.depth % 2 === 0)) {
    const holes = loops.filter(entry => entry.parent === outer).map(entry => entry.loop);
    const points = outer.loop.concat(...holes);
    for (const face of ShapeUtils.triangulateShape(outer.loop, holes)) {
      const [a, b, c] = face.map(index => points[index]);
      const winding = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
      if (winding * sign < 0) face.reverse();
      for (const index of face) {
        const point = [0, 0, 0];
        point[axis] = cut.at;
        point[u] = points[index].x;
        point[v] = points[index].y;
        positions.push(...point);
        normals.push(...normal);
      }
    }
  }
}

function cutMesh(source, cut) {
  const axis = axes[cut.axis];
  const points = source.getAttribute('position');
  const directions = source.getAttribute('normal');
  const count = source.index?.count ?? points.count;
  const positions = [], normals = [];
  const segments = [];
  let hasInterior = false, hasExterior = false;
  const vertex = index => {
    const id = source.index ? source.index.getX(index) : index;
    const point = new Vector3().fromBufferAttribute(points, id);
    return {
      point, normal: new Vector3().fromBufferAttribute(directions, id),
      distance: (point.getComponent(axis) - cut.at) * (cut.keep_positive ? 1 : -1),
    };
  };
  for (let index = 0; index < count; index += 3) {
    const triangle = [vertex(index), vertex(index + 1), vertex(index + 2)];
    hasInterior ||= triangle.some(corner => corner.distance > 0);
    hasExterior ||= triangle.some(corner => corner.distance < 0);
    const clipped = [];
    for (let edge = 0; edge < 3; edge++) {
      const a = triangle[edge], b = triangle[(edge + 1) % 3];
      if (a.distance >= 0) clipped.push(a);
      if (a.distance * b.distance < 0) {
        const t = a.distance / (a.distance - b.distance);
        clipped.push({ point: a.point.clone().lerp(b.point, t),
          normal: a.normal.clone().lerp(b.normal, t).normalize() });
      }
    }
    if (cut.cap && triangle.some(corner => corner.distance < 0)) {
      const onPlane = clipped.filter(corner => Math.abs(corner.point.getComponent(axis) - cut.at) < 1e-7);
      if (onPlane.length === 2) segments.push(onPlane.map(corner => corner.point));
    }
    for (let i = 1; i + 1 < clipped.length; i++) {
      for (const corner of [clipped[0], clipped[i], clipped[i + 1]]) {
        positions.push(...corner.point.toArray());
        normals.push(...corner.normal.toArray());
      }
    }
  }
  if (!positions.length || (!hasInterior && hasExterior)) throw Error('Cut plane removed the entire part');
  if (cut.cap && segments.length) appendCutCap(segments, axis, cut, positions, normals);
  const mesh = new BufferGeometry();
  mesh.setAttribute('position', new Float32BufferAttribute(positions, 3));
  mesh.setAttribute('normal', new Float32BufferAttribute(normals, 3));
  return mesh;
}

function solidify(source, thickness) {
  const points = source.getAttribute('position'), directions = source.getAttribute('normal');
  const count = source.index?.count ?? points.count;
  const vertices = new Map(), edges = new Map(), faces = [];
  const weld = index => {
    const id = source.index ? source.index.getX(index) : index;
    const point = new Vector3().fromBufferAttribute(points, id);
    const normal = new Vector3().fromBufferAttribute(directions, id);
    const key = point.toArray().map(value => Math.round(value * 1e5)).join(':');
    if (!vertices.has(key)) vertices.set(key, { id: vertices.size, point, normal: new Vector3() });
    const vertex = vertices.get(key);
    vertex.normal.add(normal);
    return { vertex, normal };
  };
  for (let index = 0; index < count; index += 3) {
    const face = [weld(index), weld(index + 1), weld(index + 2)];
    const [a, b, c] = face.map(corner => corner.vertex.point);
    if (new Vector3().subVectors(b, a).cross(new Vector3().subVectors(c, a)).lengthSq() < 1e-18) continue;
    faces.push(face);
    for (let edge = 0; edge < 3; edge++) {
      const from = face[edge].vertex, to = face[(edge + 1) % 3].vertex;
      const key = [Math.min(from.id, to.id), Math.max(from.id, to.id)].join(':');
      const previous = edges.get(key);
      if (previous) {
        if (previous.count !== 1 || previous.from === from) throw Error('Thickness needs an oriented manifold surface');
        previous.count++;
      } else edges.set(key, { from, to, count: 1 });
    }
  }
  if (!faces.length) throw Error('Thickness needs a surface with nonzero area');
  for (const vertex of vertices.values()) {
    if (vertex.normal.lengthSq() < 1e-12) throw Error('Thickness needs consistent surface normals');
    vertex.back = vertex.point.clone().addScaledVector(vertex.normal.normalize(), -thickness);
  }
  const positions = [], normals = [];
  const triangle = (points, directions) => {
    for (let i = 0; i < 3; i++) {
      positions.push(...points[i].toArray());
      normals.push(...directions[i].toArray());
    }
  };
  for (const face of faces) {
    triangle(face.map(corner => corner.vertex.point), face.map(corner => corner.normal));
    const back = [...face].reverse();
    triangle(back.map(corner => corner.vertex.back), back.map(corner => corner.normal.clone().negate()));
  }
  for (const { from, to, count } of edges.values()) {
    if (count !== 1) continue;
    const normal = new Vector3().subVectors(from.back, from.point)
      .cross(new Vector3().subVectors(to.back, from.point)).normalize();
    triangle([from.point, from.back, to.back], [normal, normal, normal]);
    triangle([from.point, to.back, to.point], [normal, normal, normal]);
  }
  const mesh = new BufferGeometry();
  mesh.setAttribute('position', new Float32BufferAttribute(positions, 3));
  mesh.setAttribute('normal', new Float32BufferAttribute(normals, 3));
  return mesh;
}

function buildPart(part) {
  const original = geometryFor(part.shape);
  const shell = part.thick ? solidify(original, part.thick) : original;
  let source = shell;
  if (part.cut) {
    try { source = cutMesh(shell, part.cut); }
    finally { if (part.thick) shell.dispose(); }
  }
  const positions = [], normals = [], indices = [];
  const { position, rotation, scale } = part.transform;
  const orientation = new Quaternion().setFromEuler(new Euler(...rotation.map(value => value * degrees), 'ZYX'));
  const offset = part.repeat?.offset ?? [0, 0, 0];
  const around = part.around;
  const count = around?.count ?? part.repeat?.count ?? 1;
  const center = around ? new Vector3(...around.center) : null;
  const radialAxis = around ? new Vector3().setComponent(axes[around.axis], 1) : null;
  const sourceIndices = source.index?.count ?? source.attributes.position.count;
  if (sourceIndices * count * (part.mirror ? 2 : 1) > 450_000) {
    if (source !== original) source.dispose();
    throw Error('Repeated part exceeds the mesh budget');
  }
  for (let copy = 0; copy < count; copy++) {
    for (let reflected = 0; reflected <= Number(Boolean(part.mirror)); reflected++) {
      const placement = new Vector3(...position.map((value, axis) => value + offset[axis] * copy));
      let facing = orientation;
      if (around) {
        const rotation = new Quaternion().setFromAxisAngle(radialAxis, copy / count * 2 * Math.PI);
        placement.sub(center).applyQuaternion(rotation).add(center);
        facing = rotation.multiply(orientation);
      }
      const matrix = new Matrix4().compose(
        placement,
        facing,
        new Vector3(...scale),
      );
      if (reflected) {
        const flip = [1, 1, 1];
        flip[axes[part.mirror]] = -1;
        matrix.premultiply(new Matrix4().makeScale(...flip));
      }
      const mesh = source.clone().applyMatrix4(matrix);
      const start = positions.length / 3;
      for (const value of mesh.attributes.position.array) positions.push(value);
      for (const value of mesh.attributes.normal.array) normals.push(value);
      const size = mesh.index?.count ?? mesh.attributes.position.count;
      const at = index => start + (mesh.index ? mesh.index.array[index] : index);
      for (let index = 0; index < size; index += 3) {
        indices.push(at(index), at(index + (reflected ? 2 : 1)), at(index + (reflected ? 1 : 2)));
      }
      mesh.dispose();
    }
  }
  if (source !== original) source.dispose();
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let index = 0; index < positions.length; index++) {
    const axis = index % 3, value = positions[index];
    min[axis] = Math.min(min[axis], value);
    max[axis] = Math.max(max[axis], value);
  }
  return {
    positions: new Float32Array(positions), normals: new Float32Array(normals),
    indices: new Uint32Array(indices),
    bounds: { center: min.map((value, axis) => (value + max[axis]) / 2),
      size: min.map((value, axis) => max[axis] - value) },
  };
}

let latestRequest;
self.onmessage = async ({ data: { id, parts, known } }) => {
  latestRequest = id;
  try {
    const unchanged = new Map(known);
    let triangles = 0;
    for (const [index, part] of parts.entries()) {
      const key = JSON.stringify([part.shape, part.transform, part.mirror, part.repeat, part.around, part.cut, part.thick]);
      if (part.shape.kind === 'group') {
        self.postMessage({ id, part: { ...part, key } });
        continue;
      }
      if (unchanged.has(key)) {
        triangles += unchanged.get(key);
        if (triangles > 150_000) throw Error('Figure exceeds the mesh budget');
        self.postMessage({ id, part: { ...part, key } });
        continue;
      }
      const mesh = buildPart(part);
      triangles += mesh.indices.length / 3;
      if (triangles > 150_000) throw Error('Figure exceeds the mesh budget');
      self.postMessage({ id, part: { ...part, key, ...mesh } },
        [mesh.positions.buffer, mesh.normals.buffer, mesh.indices.buffer]);
      unchanged.set(key, mesh.indices.length / 3);
      if (index + 1 < parts.length) {
        // Let a newer stream prefix take over between expensive meshes.
        await new Promise(resolve => setTimeout(resolve, 0));
        if (id !== latestRequest) return;
      }
    }
    self.postMessage({ id, done: true });
  } catch (error) {
    self.postMessage({ id, error: String(error.message || error) });
  }
};
