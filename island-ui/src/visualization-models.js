// Composer eligibility only. This does not change the scene planner or renderer,
// and is not a claim of equal modeling quality across providers.
const normalize = value => String(value || '').toLowerCase().replace(/[^a-z0-9.]+/g, ' ').trim();
const branded = /\b(?:gpt\s*\d|kimi|deepseek|qwen\s*\d|glm\s*\d|claude|gemini)/;

export function supports3DVisualization(id, name = '', group = '') {
  const canonical = normalize(id), label = normalize(name);
  // A known model ID takes precedence over a friendly label. Opaque ACP IDs
  // may use the catalog label/provider group instead.
  const model = branded.test(canonical) ? canonical : branded.test(label) ? label : normalize(`${group} ${label || canonical}`);
  if (/\b(?:distill\w*|embedding\w*|rerank\w*|mini|nano|lite|haiku|air)\b/.test(model)) return false;

  const gpt = model.match(/\bgpt\s*6(\.\d+)?\b(.*)$/);
  if (gpt) return /\b(?:sol|astra)\b/.test(gpt[2]) || !gpt[1] && !gpt[2].trim();
  if (/\bkimi\s+(?:k(?:3|2\.(?:7|8))\b|for\s+coding\b)/.test(model)) return true;

  // Full-size reasoning/coding families available through OpenCode catalogs.
  // Preserve version boundaries: V3.2 is different from V3 or a distilled R1.
  if (/\bdeepseek\s+(?:v(?:3\.2|4(?:\.\d+)?)\b|r1\b|chat\b|reasoner\b|flash\b)/.test(model)) return !/\b(?:1\.5|7|8|14|32|70)b\b/.test(model);
  if (/\bqwen\s*3(?:\s+coder\b|\.7\s+max\b)/.test(model)) return true;
  if (/\bglm\s*(?:4\.7|5(?:\.\d+)?)\b/.test(model)) return true;
  if (/\bclaude\s+(?:sonnet|opus)\s+(?:4[. ](?:6|7|8)|5(?:\.\d+)?)\b/.test(model)) return true;
  return /\bgemini\s*3(?:\.1)?\s+pro\b/.test(model);
}
