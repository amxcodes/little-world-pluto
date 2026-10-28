import test from 'node:test';
import assert from 'node:assert/strict';
import { rangeEdge, rangeValueAt } from '../src/split-range-geometry.js';

test('drag reaches both endpoints and follows pointer position', () => {
  assert.equal(rangeValueAt(-100, 100, 200, 0, 100, 1), 0);
  assert.equal(rangeValueAt(200, 100, 200, 0, 100, 1), 50);
  assert.equal(rangeValueAt(400, 100, 200, 0, 100, 1), 100);
  assert.equal(rangeValueAt(200, 100, 200, 20, 100, 5), 60);
});

test('segments narrow only within the outer fifths', () => {
  assert.equal(rangeEdge(0), 0);
  assert.equal(rangeEdge(10), .5);
  assert.equal(rangeEdge(20), 1);
  assert.equal(rangeEdge(80), 1);
  assert.equal(rangeEdge(90), .5);
  assert.equal(rangeEdge(100), 0);
});
