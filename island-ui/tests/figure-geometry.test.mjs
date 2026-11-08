import { readFileSync } from 'node:fs';
import { figureAssemblyGaps } from '../src/figure-motion.js';
import assert from 'node:assert/strict';
import test from 'node:test';
import { BufferAttribute, BufferGeometry, Mesh, MeshBasicMaterial, Raycaster, Vector3 } from 'three';

let messages = [];
globalThis.self = { postMessage: message => messages.push(message) };
await import('../src/figure-worker.js');

function cook(shape, cut, thick) {
  messages = [];
  self.onmessage({ data: { id: 1, known: [], parts: [{
    id: 'part', shape, cut, thick,
    transform: { position: [0, 0, 0], rotation: [0, 0, 0], scale: [1, 1, 1] },
  }] } });
  assert.equal(messages.at(-1).done, true, JSON.stringify(messages.at(-1)));
  const part = messages[0].part;
  assert(part.positions.every(Number.isFinite));
  assert(part.normals.every(Number.isFinite));
  return part;
}

test('lathe recipes can describe a rounded partial revolution at a chosen resolution', () => {
  const shape = { kind: 'lathe', profile: [[0, -1], [1, -1], [1, 1], [0, 1]] };
  const full = cook(shape);
  const partial = cook({ ...shape, arc: 90, segments: 12, bevel: .15 });
  assert(full.bounds.size[0] > 1.9);
  assert(partial.bounds.size[0] <= 1);
  assert(partial.positions.length > full.positions.length);
});

test('a newer prefix supersedes the old build between cooked parts', async () => {
  const part = { shape: { kind: 'box', size: [1, 1, 1] },
    transform: { position: [0, 0, 0], rotation: [0, 0, 0], scale: [1, 1, 1] } };
  messages = [];
  const first = self.onmessage({ data: { id: 80, known: [], parts: [part, part] } });
  const mesh = messages[0].part;
  await self.onmessage({ data: { id: 81, known: [[mesh.key, mesh.indices.length / 3]], parts: [part] } });
  await first;
  assert.deepEqual(messages.map(message => message.id), [80, 81, 81]);
  assert.equal(messages[1].part.positions, undefined);
  assert.equal(messages[2].done, true);
});

function hit(part, x) {
  const geometry = new BufferGeometry()
    .setAttribute('position', new BufferAttribute(part.positions, 3))
    .setIndex(new BufferAttribute(part.indices, 1));
  const material = new MeshBasicMaterial();
  try {
    return new Raycaster(new Vector3(x, 5, 0), new Vector3(0, -1, 0))
      .intersectObject(new Mesh(geometry, material));
  } finally {
    geometry.dispose();
    material.dispose();
  }
}

test('rounded cylinders, cones and sleeves preserve height, facing and a hollow bore', () => {
  for (const shape of [
    { kind: 'cylinder', radius: 2, height: 4, bevel: .3 },
    { kind: 'cylinder', radius: 2, top_radius: 0, height: 4, bevel: .3 },
    { kind: 'sleeve', radius: 2, inner_radius: 1, height: 4, bevel: .3 },
  ]) {
    const part = cook(shape);
    assert.equal(part.bounds.size[1], 4);
    assert(hit(part, 1.5).length > 0);
    assert.equal(hit(part, 0).length === 0, shape.kind === 'sleeve');
    const diagonalNormals = [];
    for (let i = 0; i < part.normals.length; i += 3) {
      const y = Math.abs(part.normals[i + 1]);
      if (y > .01 && y < .99) diagonalNormals.push(y.toFixed(3));
    }
    assert(new Set(diagonalNormals).size > 2, 'rounded edge needs several normal angles');
  }
});

test('solidification closes a sampled panel with correctly oriented boundary walls', () => {
  const part = cook({ kind: 'surface', grid: [
    [[-1, 0, -1], [1, 0, -1]],
    [[-1, 0, 1], [1, 0, 1]],
  ] }, undefined, .2);
  assert(Math.abs(part.bounds.size[1] - .2) < 1e-7);
  assert.equal(hit(part, 0)[0]?.point.y, 0);
  const edges = new Map();
  for (let i = 0; i < part.indices.length; i += 3) {
    const face = Array.from(part.indices.slice(i, i + 3), index =>
      Array.from(part.positions.slice(index * 3, index * 3 + 3), value => value.toFixed(5)).join(','));
    for (let edge = 0; edge < 3; edge++) {
      const from = face[edge], to = face[(edge + 1) % 3];
      const key = [from, to].sort().join('|');
      const entry = edges.get(key) ?? { count: 0, direction: 0 };
      entry.count++;
      entry.direction += from < to ? 1 : -1;
      edges.set(key, entry);
    }
  }
  for (const edge of edges.values()) {
    assert.equal(edge.count, 2);
    assert.equal(edge.direction, 0);
  }
});

test('a capped cross section closes a solid while retaining the sleeve bore', () => {
  const cut = { axis: 'y', at: 0, cap: true };
  const solid = cook({ kind: 'box', size: [4, 4, 4] }, cut);
  assert.equal(hit(solid, 0)[0]?.point.y, 0);
  const sleeve = cook({ kind: 'sleeve', radius: 2, inner_radius: 1, height: 4, bevel: .2 }, cut);
  assert.equal(hit(sleeve, 0).length, 0);
  assert.equal(hit(sleeve, 1.5)[0]?.point.y, 0);
});

test('a solidified panel can be capped across its thickness', () => {
  const part = cook({ kind: 'surface', grid: [
    [[-1, 0, -1], [1, 0, -1]],
    [[-1, 0, 1], [1, 0, 1]],
  ] }, { axis: 'x', at: 0, cap: true }, .2);
  assert.equal(part.bounds.size[0], 1);
  const geometry = new BufferGeometry()
    .setAttribute('position', new BufferAttribute(part.positions, 3))
    .setIndex(new BufferAttribute(part.indices, 1));
  const material = new MeshBasicMaterial();
  try {
    const hits = new Raycaster(new Vector3(2, -.1, 0), new Vector3(-1, 0, 0))
      .intersectObject(new Mesh(geometry, material));
    assert.equal(hits[0]?.point.x, 0);
  } finally {
    geometry.dispose();
    material.dispose();
  }
});

test('solidification welds a dome seam and pole without closing the open interior', () => {
  const part = cook({ kind: 'sphere', radius: 2, dome: true }, undefined, .2);
  assert(Math.abs(hit(part, 0)[0]?.point.y - 2) < 1e-6);
  const geometry = new BufferGeometry()
    .setAttribute('position', new BufferAttribute(part.positions, 3))
    .setIndex(new BufferAttribute(part.indices, 1));
  const material = new MeshBasicMaterial();
  try {
    const hits = new Raycaster(new Vector3(0, .1, 0), new Vector3(0, 1, 0))
      .intersectObject(new Mesh(geometry, material));
    assert(Math.abs(hits[0]?.point.y - 1.8) < 1e-6);
  } finally {
    geometry.dispose();
    material.dispose();
  }
});

test('organic blobs are reproducible, vary with seed and retain bounded radii', () => {
  const first = cook({ kind: 'blob', radius: 2, seed: 7, lumps: .2 });
  const again = cook({ kind: 'blob', radius: 2, seed: 7, lumps: .2 });
  const other = cook({ kind: 'blob', radius: 2, seed: 8, lumps: .2 });
  assert.deepEqual(first.positions, again.positions);
  assert.notDeepEqual(first.positions, other.positions);
  const defaulted = cook({ kind: 'blob', radius: 2, seed: null, lumps: null });
  const radii = [];
  for (let i = 0; i < defaulted.positions.length; i += 3) {
    const radius = Math.hypot(...defaulted.positions.slice(i, i + 3));
    assert(radius > 2 * .78 && radius < 2 * 1.22);
    radii.push(radius);
  }
  assert(Math.max(...radii) - Math.min(...radii) > .1);
});

test('a blob stick can taper smoothly between different endpoint radii', () => {
  const part = cook({ kind: 'blob', blend: 0, field: [
    { kind: 'stick', from: [0, -1, 0], to: [0, 1, 0], radius: .8, to_radius: .2 },
  ] });
  const geometry = new BufferGeometry()
    .setAttribute('position', new BufferAttribute(part.positions, 3))
    .setIndex(new BufferAttribute(part.indices, 1));
  const material = new MeshBasicMaterial();
  try {
    const radiusAt = y => new Raycaster(new Vector3(3, y, 0), new Vector3(-1, 0, 0))
      .intersectObject(new Mesh(geometry, material))[0]?.point.x;
    assert(radiusAt(-.8) > radiusAt(.8) * 1.6);
  } finally {
    geometry.dispose();
    material.dispose();
  }
});

test('a cut removing the entire volume does not publish a zero thickness remnant', () => {
  messages = [];
  self.onmessage({ data: { id: 1, known: [], parts: [{
    shape: { kind: 'box', size: [2, 2, 2] },
    transform: { position: [0, 0, 0], rotation: [0, 0, 0], scale: [1, 1, 1] },
    cut: { axis: 'y', at: -1, cap: true },
  }] } });
  assert.match(messages.at(-1).error, /removed the entire part/);
});


test('session 79 reports disconnected assemblies using the actual cooked geometry', async () => {
  const saved = JSON.parse(readFileSync(new URL('../../app/test-fixtures/session79-presentation.json', import.meta.url), 'utf8'));
  const figure = saved.elements.find(element => element.type === 'figure_scene');
  messages = [];
  await self.onmessage({ data: { id: 79, known: [], parts: figure.parts } });
  assert.equal(messages.at(-1).done, true, JSON.stringify(messages.at(-1)));
  const boundsByKey = new Map();
  const cooked = messages.flatMap(({part}) => {
    if (!part) return [];
    if (part.bounds) boundsByKey.set(part.key, part.bounds);
    return [{...part, bounds: boundsByKey.get(part.key)}];
  });
  const gaps = figureAssemblyGaps(cooked, figure.steps.length - 1);
  assert.ok(gaps.some(gap => gap.gap > .15 && gap.stage > 0), JSON.stringify(gaps));
});
