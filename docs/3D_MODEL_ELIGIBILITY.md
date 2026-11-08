# Composer model eligibility — 2026-11-08

This policy controls whether the Agents composer offers the opt-in **3D Visualize**
button. It does not change scene planning, recipes, repair, rendering, animation,
reference gathering, or ACP transports. Coding capability is evidence for
eligibility, not a measured guarantee of modeling quality.

## Eligible families

| Family | Eligible catalog IDs/names |
| --- | --- |
| Existing GPT | GPT-6; GPT-6.x Sol and Astra |
| Existing Kimi | K2.7, K2.8, K3, Kimi for Coding |
| DeepSeek | V3.2, V4/V4.x, full R1; provider aliases `deepseek-chat`, `deepseek-reasoner`, `deepseek-flash` |
| Qwen | Qwen3-Coder, including Next/Plus and full Instruct variants; Qwen3.7-Max |
| GLM | GLM-4.7 and GLM-5.x |
| Claude | Sonnet/Opus 4.6–4.8 and 5.x |
| Gemini | Gemini 3 Pro and 3.1 Pro |

Mini, nano, lite, air, Haiku, distilled, embedding and reranking variants are
excluded. Small DeepSeek derivatives with explicit 1.5/7/8/14/32/70B sizes are
also excluded. Unknown models remain ineligible.

The authoritative model ID is checked first, including routed provider prefixes
and version punctuation. Friendly names and provider groups are a fallback for
opaque ACP option IDs. A friendly label cannot upgrade a known unsupported ID.
No extra network request or model-catalog refresh is added to this check.

The toggle remains restricted to the Agents task composer. Switching provider,
model or session clears its opt-in, and Ask/wake surfaces exclude it. Provider
availability, sign-in and selected model configuration continue to come from ACP.
Alias targets may change at the provider; recheck them when updating this policy.

## Evidence used for the additions

- [DeepSeek API and current model aliases](https://api-docs.deepseek.com/quick_start/agent_integrations/) and [official OpenCode integration](https://api-docs.deepseek.com/quick_start/agent_integrations/opencode/).
- [Qwen3-Coder agentic coding](https://qwenlm.github.io/blog/qwen3-coder/) and [Qwen3.7-Max](https://qwen.ai/blog?id=qwen3.7).
- [GLM-4.7 model documentation](https://docs.api.nvidia.com/nim/reference/z-ai-glm4-7) and [GLM-5.3 coding workflow](https://zcode.z.ai/en/docs/welcome).
- [Claude Sonnet 4.6](https://www.anthropic.com/news/claude-sonnet-4-6), [Opus 4.6](https://www.anthropic.com/news/claude-opus-4-6), and [Anthropic model catalog](https://github.com/anthropics/skills/blob/main/skills/claude-api/shared/models.md).
- [Gemini 3.1 Pro model card](https://deepmind.google/models/model-cards/gemini-3-1-pro) and [structured outputs/function calling](https://ai.google.dev/gemini-api/docs/models/gemini-3.1-pro-preview).

These sources describe coding/reasoning and integration capabilities. The
regressions verify local eligibility and UI isolation; they do not run or compare
live generations from the newly eligible providers.

## Navigation shadow fix

The light glass navigation lens cast an outer shadow into an ancestor with
horizontal scrolling. That ancestor also clips vertical overflow, leaving a
straight cutoff beneath the pill. Light Liquid Glass and Hyper Glass navigation
now use an entirely inset rim. Scrolling, lens motion and the remaining material
shadows are preserved.

## Validation

- Model-family, alias, version, size and misleading-label regression tests.
- Rendered Agents/Ask/wake isolation checks for both ACP providers.
- Six real-browser contexts for light Liquid/Hyper Glass navigation, held lenses,
  live model eligibility and clearing opt-in when changing models.
- Browser screenshots and report: `target/ui-review/navigation-models/`.
- All 24 protected pipeline/legacy-stylesheet hashes remain unchanged.
- All 558 existing Standard/Liquid browser checks passed.

Local optimized build: `target/hyper-glass/Pluto-UI-model-support.exe`, signed
with the existing Pluto certificate without an external timestamp. The installed
copy was not replaced and nothing was pushed to GitHub. Quit the running copy
before launching this executable.

SHA-256: `ADC22B09CE14DF76209D8BAAE44114E3D530769CB5037307860780ACFE5212D7`.
