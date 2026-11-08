import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

test('obsolete engine selections are discarded while wake preferences survive', () => {
  const source = fs.readFileSync(new URL('../src/agents-ui.jsx', import.meta.url), 'utf8');
  const functions = source.slice(source.indexOf('export function readAgentPreferences'), source.indexOf('function useAgentPreferences'))
    .replaceAll('export function', 'function');
  const values = new Map();
  const input = { wakeEngine: 'obsolete-engine', wakePhrase: 'Hey Pluto', wakeSensitivity: 'high', providerSetupComplete: true };
  values.set('island.agent-preferences', JSON.stringify(input));
  const context = vm.createContext({
    defaults: { provider: {} }, defaultAskInstructions: '',
    localStorage: { getItem: key => values.get(key), setItem: (key, value) => values.set(key, value) },
    window: { dispatchEvent() {} }, CustomEvent: class {},
  });
  vm.runInContext(functions, context);
  const loaded = vm.runInContext('readAgentPreferences()', context);
  assert.equal('wakeEngine' in loaded, false);
  assert.equal(loaded.wakePhrase, input.wakePhrase);
  assert.equal(loaded.wakeSensitivity, input.wakeSensitivity);
  context.input = input;
  vm.runInContext('saveAgentPreferences(input)', context);
  const saved = JSON.parse(values.get('island.agent-preferences'));
  assert.equal('wakeEngine' in saved, false);
  assert.equal(saved.wakePhrase, input.wakePhrase);
  assert.equal(input.wakeEngine, 'obsolete-engine');
});
