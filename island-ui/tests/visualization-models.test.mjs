import test from 'node:test';
import assert from 'node:assert/strict';
import {supports3DVisualization as eligible} from '../src/visualization-models.js';

test('existing GPT and Kimi model eligibility survives provider-prefixed IDs',()=>{
 for(const id of ['openai/gpt-6','gpt6','openai/gpt-6-sol','gpt-6.1-sol','gpt-6.1-astra','moonshotai/kimi-k2.7','kimi-k2.8','kimi-k3','kimi-for-coding'])assert.ok(eligible(id),id);
 for(const id of ['gpt-6-mini','gpt-6.1-terra','gpt-6.1','gpt-5','kimi-k2','kimi-k2.5'])assert.equal(eligible(id),false,id);
});
test('strong OpenCode families and their routed IDs qualify',()=>{
 for(const id of ['deepseek/deepseek-v3.2','deepseek/deepseek-v4-pro','deepseek-v4.1-flash','deepseek-flash','deepseek-chat','deepseek-reasoner','deepseek/deepseek-r1-0528','qwen/qwen3-coder-480b-a35b-instruct','qwen3-coder-next','qwen3-coder-plus','qwen3.7-max','z-ai/glm-4.7','zai-org/glm-5','glm-5.3','anthropic/claude-sonnet-4-6','anthropic/claude-opus-4.6','claude-sonnet-5','google/gemini-3-pro-preview','gemini-3.1-pro-preview'])assert.ok(eligible(id),id);
});
test('small, distilled, embedding, older and unrelated models stay ineligible',()=>{
 for(const id of ['',undefined,'deepseek-v3','deepseek-v3.1','deepseek-r1-distill-qwen-32b','deepseek-r1-0528-qwen3-8b','qwen3-embedding-8b','qwen2.5-coder-7b','qwen3.7-plus','glm-4.7-flash-lite','glm-4.5-air','claude-haiku-4.5','claude-sonnet-3.7','gemini-3-flash','gemini-3.1-pro-image-nano','unrelated-model'])assert.equal(eligible(id),false,String(id));
});
test('catalog labels are a fallback, never an upgrade of a known unsupported ID',()=>{
 assert.ok(eligible('opaque-model-17','DeepSeek V4 Pro','DeepSeek'));
 assert.ok(eligible('opaque-model-18','V4 Pro','DeepSeek'));
 assert.equal(eligible('deepseek-v3','DeepSeek V4 Pro'),false);
 assert.equal(eligible('gpt-6-mini','GPT-6 Sol'),false);
 assert.equal(eligible('model-0','Unknown','OpenAI'),false);
});
