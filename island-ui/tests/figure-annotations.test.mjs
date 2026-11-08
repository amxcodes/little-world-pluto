import assert from 'node:assert/strict';
import test from 'node:test';
import { Matrix4, OrthographicCamera, Scene } from 'three';
import { createFigureAnnotations, figureAnnotations, layoutFigureLabel } from '../src/figure-annotations.js';

test('labels resolve fixed points and part centers while waiting for streamed parts', () => {
  const parts = [{ id: 'cab', label: 'Cab', bounds: { center: [1, 2, 3] } }];
  assert.deepEqual(figureAnnotations(undefined, parts, 'cab'),
    [{ text: 'Cab', part: 'cab', point: [1, 2, 3] }]);
  const labels = [{ text: 'Origin', point: [0, 0, 0] }, { text: 'Wheel', part: 'wheel' }];
  assert.equal(figureAnnotations(labels, parts).length, 1);
  assert.equal(figureAnnotations(labels, [...parts, { id: 'wheel', bounds: { center: [2, 0, 1] } }]).length, 2);
});

test('unlabelled figures show major part names before any selection', () => {
  const mesh = (id, label, parent, size) => ({id, label, parent, bounds: {center:[0,0,0], size:[size,1,1]}});
  const parts = [mesh('small', 'Seam', 'body', .1), mesh('shell', 'Cab', 'body', 4),
    mesh('axle', 'Axle', 'chassis', 2), mesh('head', 'Head', 'upper', 1)];
  assert.deepEqual(figureAnnotations([], parts).map(label => label.text), ['Cab','Axle','Head']);
  assert.deepEqual(figureAnnotations([{text:'Chosen',part:'small'}], parts).map(label=>label.text), ['Chosen']);
});

test('WebView labels reuse textures, follow poses and visibility, and release resources', () => {
  globalThis.document = { createElement: () => ({
    getContext: () => ({
      measureText: text => ({ width: text.length * 8 }),
      scale() {}, beginPath() {}, roundRect() {}, fill() {}, fillText() {},
    }),
  }) };
  const scene = new Scene();
  const camera = new OrthographicCamera(-1, 1, 1, -1, .01, 100);
  camera.position.z = 5;
  camera.updateMatrixWorld();
  const layer = createFigureAnnotations(scene, camera);
  const labels = [{ text: 'Part', part: 'p', point: [0, 0, 0] }];
  const poses = new Map([['p', new Matrix4().makeTranslation(.5, 0, 0)]]);
  layer.draw(labels, poses, new Map([['p', true]]), 100);
  const sprite = scene.children[0].children[0];
  const anchor = scene.children[1].children[0].geometry.getAttribute('position');
  assert(Math.abs(anchor.getX(0) - .5) < 1e-8);
  assert.equal(sprite.visible, true);
  layer.draw(labels, poses, new Map([['p', false]]), 100);
  assert.equal(scene.children[0].children[0], sprite);
  assert.equal(sprite.visible, false);
  let disposed = 0;
  sprite.material.map.addEventListener('dispose', () => disposed++);
  sprite.material.addEventListener('dispose', () => disposed++);
  layer.dispose();
  assert.equal(disposed, 2);
  assert.equal(scene.children.length, 0);
});

test('nearby label anchors produce separate boxes inside the viewport', () => {
  const boxes = [];
  for (let i = 0; i < 5; i++) layoutFigureLabel([160, 100], 100, 320, 220, boxes);
  for (const [index, a] of boxes.entries()) {
    assert(a[0] >= 4 && a[1] >= 4 && a[2] <= 316 && a[3] <= 216);
    for (const b of boxes.slice(index + 1)) {
      assert(a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]);
    }
  }
});
