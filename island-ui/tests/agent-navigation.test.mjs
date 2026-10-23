import test from 'node:test';
import { build } from 'esbuild';
import { createRequire } from 'node:module';
import vm from 'node:vm';

test('Ask, wake answers, Agents and settings render with both providers', async () => {
  const result = await build({
    stdin: {
      resolveDir: new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'),
      loader: 'jsx',
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import assert from 'node:assert/strict';
        import { AgentsWorkspace, AgentSettings } from './src/agents-ui.jsx';
        globalThis.window = { islandAgentPreferenceBootstrap: {} };
        globalThis.localStorage = { getItem: () => 'Conversation context', setItem() {} };
        const common = { Icon: () => null, send() {}, renderAnswer: text => <p>{text}</p> };
        for (const provider of ['opencode', 'codex']) {
          window.islandAgentPreferenceBootstrap = { defaultProvider: provider, providerSetupComplete: true };
          for (const mode of [{ askMode: true }, { voiceSurface: true }, {}]) {
            for (const loaded of [false, true]) {
              const session = { id: 's1', title: 'Conversation', ready: true, configOptions: [], commands: [] };
              const agent = loaded ? {
                connected: true, selectedId: 's1', sessions: [session],
                response: { sessionId: 's1', text: 'Here is your answer.', presentation: { version: 1, elements: [{ type: 'heading', text: 'VISUAL_ONLY_SENTINEL' }] } },
                permissions: [{ id: 'p1', ownerSessionId: 's1', action: 'Read file', options: [{ optionId: 'yes', name: 'Allow', kind: 'allow_once' }] }],
                questions: [{ id: 'q1', ownerSessionId: 's1', mode: 'url', url: 'https://example.com', message: 'Sign in' }],
              } : {};
              const state = { agents: { [provider]: agent }, speechSettings: { configured: true } };
              const html = renderToStaticMarkup(<AgentsWorkspace {...common} {...mode} state={state}/>);
              assert.ok(html.length > 0);
              if (loaded && mode.voiceSurface) assert.doesNotMatch(html, /VISUAL_ONLY_SENTINEL/);
              if (loaded && !mode.voiceSurface) assert.match(html, /VISUAL_ONLY_SENTINEL/);
              if (mode.askMode || mode.voiceSurface) {
                assert.match(html, /voice-answer-surface/);
                if (loaded) { assert.match(html, /Here is your answer/); assert.match(html, /Approval needed/); }
                const recording = renderToStaticMarkup(<AgentsWorkspace {...common} {...mode} state={state} dictateActive/>);
                if (mode.askMode) assert.match(recording, /Microphone/);
                else { assert.doesNotMatch(recording, />Voice<|>Listening<|>Session</); }
                if (mode.askMode) assert.doesNotMatch(recording, /voice-eyes/);
              }
            }
          }
          assert.match(renderToStaticMarkup(<AgentSettings {...common} state={{}}/>), /Providers/);
          const waking = renderToStaticMarkup(<AgentsWorkspace {...common} voiceSurface state={{ wakeWord: { revision: 101, status: 'triggered' }, agents: {} }}/>);
          assert.match(waking, /data-layout-height="34"/);
          assert.doesNotMatch(waking, /Ask a question to begin/);
          const thinking = renderToStaticMarkup(<AgentsWorkspace {...common} voiceSurface state={{ agents: {} }}/>);
          assert.match(thinking, /data-phase="thinking"/);
          assert.match(thinking, /data-layout-height="34"/);
        }
        localStorage.getItem = () => { throw new Error('Storage unavailable'); };
        assert.match(renderToStaticMarkup(<AgentsWorkspace {...common} askMode state={{}}/>), /voice-answer-surface/);
      `,
    },
    bundle: true, platform: 'node', format: 'cjs', write: false,
  });
  const require = createRequire(import.meta.url);
  const context = { require, console, process, Buffer, TextEncoder, TextDecoder, setTimeout, clearTimeout, setImmediate, clearImmediate, queueMicrotask, performance, URL, ReadableStream, TransformStream, AbortController };
  vm.runInNewContext(result.outputFiles[0].text, context, { filename: 'agent-navigation.cjs' });
});

