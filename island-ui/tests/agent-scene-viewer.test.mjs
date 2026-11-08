import assert from 'node:assert/strict';
import test from 'node:test';
import { Vector3 } from 'three';
import { orbitPosition, figureReviewViews } from '../src/agent-scene-viewer.js';

test('Figure orbit keeps the Y axis upright and looks down from above', () => {
  const center = new Vector3(0, 2, 0);
  const camera = orbitPosition(center, 2, 36, .48, 'y');
  assert.ok(camera.y > center.y);
  assert.ok(camera.z > center.z);
  const top = orbitPosition(center, 2, 36, 1.45, 'y');
  assert.ok(top.y > camera.y);
});

test('legacy procedural scenes retain the Z-up orbit', () => {
  const center = new Vector3(0, 0, 2);
  const camera = orbitPosition(center, 2, 36, .48);
  assert.ok(camera.z > center.z);
});

test('review image includes the starting and ending transformation poses', () => {
  const start = new Map([['cab', 'start']]);
  const end = new Map([['cab', 'end']]);
  const middle = new Map([['cab', 'middle']]);
  const views = figureReviewViews('y', 2, start, end, middle);
  assert.equal(views.length, 9);
  assert.deepEqual(views.map(view => view[3]), [start, start, start, middle, middle, middle, end, end, end]);
  assert.deepEqual(views.map(view => view[4]), [0, 0, 0, 1, 1, 1, 2, 2, 2]);
});
