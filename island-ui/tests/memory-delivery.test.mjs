import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../src/main.jsx', import.meta.url), 'utf8');
test('routine Convert snapshots do not close Settings after its first open', () => {
  const start = source.indexOf('    window.islandReceive = next =>');
  const body = source.slice(start, source.indexOf('      let update = next;', start));
  let settings = true;
  const context = { window: {}, setSettingsOpen: value => { settings = value; } };
  vm.runInNewContext(`let receivedPage = 'Home'; ${body} };`, context);
  context.window.islandReceive({ page: 'Convert' });
  assert.equal(settings, false, 'a new native conversion route may dismiss Settings');
  settings = true;
  for (let i = 0; i < 100; i++) {
    context.window.islandReceive({ page: 'Convert' });
    context.window.islandReceive({ battery: i });
  }
  assert.equal(settings, true, 'unchanged and partial snapshots preserve the overlay');
});

test('shell-only updates preserve agent state identity and pending approvals', () => {
  const start = source.indexOf('      setState(previous => {', source.indexOf('window.islandReceive = next =>'));
  const code = source.slice(start, source.indexOf('      if (Array.isArray(update.homeActions))', start));
  const agents = { opencode: { response: { text: 'Streaming answer' }, permissions: [{ id: 'approval' }] } };
  const catalog = { models: [{ key: 'model' }] };
  let state = { agents, opencode: catalog, shelfRevision: 1, shelfIcons: [] };
  const context = { update: { page: 'Ask', shelfRevision: 1 }, mergeMedia: previous => previous, setState: fn => { state = fn(state); } };
  for (let i = 0; i < 100; i++) vm.runInNewContext(code, context);
  assert.equal(state.agents, agents);
  assert.equal(state.opencode, catalog);
  assert.equal(state.agents.opencode.permissions[0].id, 'approval');
  const replacement = { opencode: { response: { text: 'Finished' }, permissions: [] } };
  context.update = { agents: replacement, shelfRevision: 1 };
  vm.runInNewContext(code, context);
  assert.equal(state.agents, replacement);
  assert.equal(state.opencode, catalog);
});

test('settings warms only the configured provider and skips an existing connection', () => {
  const agents = readFileSync(new URL('../src/agents-ui.jsx', import.meta.url), 'utf8');
  const start = agents.indexOf('  useEffect(() => { const id = preferences.defaultProvider');
  assert.ok(start >= 0);
  const code = agents.slice(start, agents.indexOf('\n', start));
  for (const provider of ['opencode', 'codex']) {
    const sent = [];
    const context = { preferences: { defaultProvider: provider }, providers: {}, send: {}, sendAgent: (_, id, action) => sent.push([id, action]), useEffect: fn => fn() };
    vm.runInNewContext(code, context);
    assert.deepEqual(sent, [[provider, 'connect']]);
    context.providers[provider] = { connected: true };
    vm.runInNewContext(code, context);
    assert.equal(sent.length, 1);
    context.providers[provider] = { installing: true };
    vm.runInNewContext(code, context);
    assert.equal(sent.length, 1);
  }
});
