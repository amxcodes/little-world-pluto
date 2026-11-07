import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { transformSync } from 'esbuild';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

const source = new URL('../src/agent-construction.jsx', import.meta.url);
const require = createRequire(source);
const module = { exports: {} };
const transformed = transformSync(readFileSync(source, 'utf8'), { loader: 'jsx', format: 'cjs' });
new Function('require', 'module', 'exports', transformed.code)(require, module, module.exports);

test('the validated construction fixture renders selectable mirrored parts in the answer card', () => {
  const ops = [
    { op: 'box', id: 'body', label: 'Housing', detail: 'Encloses electronics.', position: [-2, -1, 0], size: [4, 2, .5] },
    { op: 'box', id: 'left', label: 'Left antenna', detail: 'Wireless antenna.', position: [-1.7, .5, .4], size: [.2, .2, 2] },
    { op: 'mirror', id: 'right', label: 'Right antenna', detail: 'Second antenna.', source: 'left', axis: 'x' },
  ];
  const parts = module.exports.resolveConstructionOps(ops);
  assert.deepEqual(parts[2].position, [1.5, .5, .4]);
  const html = renderToStaticMarkup(React.createElement(module.exports.ConstructionScene, { label: 'Router', ops }));
  assert.match(html, /Rotatable schematic of 3 selectable parts/);
  assert.equal((html.match(/data-part-index=/g) || []).length >= 3, true);
  for (const label of ['Housing', 'Left antenna', 'Right antenna']) assert.match(html, new RegExp(label));
  assert.match(html, /Rotate left/);
  assert.match(html, /Rotate right/);
});
