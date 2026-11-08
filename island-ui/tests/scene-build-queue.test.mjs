import test from 'node:test';
import assert from 'node:assert/strict';
import { createSceneBuildQueue } from '../src/scene-build-queue.js';

test('superseded revisions never publish and engine builds remain sequential', async () => {
  let release;
  const blocked = new Promise(resolve => { release = resolve; });
  const started = [], published = [];
  let active = 0;
  const queue = createSceneBuildQueue(async request => {
    assert.equal(++active, 1);
    started.push(request.id);
    if (request.id === 1) await blocked;
    --active;
    return { parts: [request.id] };
  }, result => published.push(result));
  const first = queue({ id: 1 });
  await queue({ id: 2 });
  await queue({ id: 3 });
  release();
  await first;
  assert.deepEqual(started, [1, 3]);
  assert.deepEqual(published, [{ id: 3, parts: [3] }]);
});

test('a failed build does not poison subsequent revisions', async () => {
  const published = [];
  const queue = createSceneBuildQueue(async request => {
    if (request.id === 1) throw Error('bad profile');
    return { parts: ['fixed'] };
  }, result => published.push(result));
  await queue({ id: 1 });
  await queue({ id: 2 });
  assert.deepEqual(published, [{ id: 1, error: 'bad profile' }, { id: 2, parts: ['fixed'] }]);
});
