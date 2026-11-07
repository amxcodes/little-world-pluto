# Agent construction visuals: working checkpoints

Pluto keeps Markdown and all registered answer components. Construction scenes use the same `island-ui` fence, Rust validation gateway, and answer card. The previous whiteboard and assembly components remain readable while the replacement is evaluated.

## Current checkpoint: bounded solids and mirroring

Try this in Agents: **“Show a simplified 3D construction view of a home Wi-Fi router with a housing and two antennas. Label the parts. Keep it schematic.”**

Expected: prose plus one selectable construction view in the answer card. Drag the view or use the rotation buttons; select each part to read its detail. A text-only request should show the explanation without the visual. The agent may omit a visual when it cannot represent the subject honestly.

Automated evidence: `cargo test --offline --locked --test agent_visual_contract`, `node --test island-ui/tests/agent-construction.test.mjs`, `npm --prefix island-ui run build`, and `cargo check --offline --locked --features product-app --bin pluto`. These prove the contract, fallback, mirrored coordinates, component markup, and compilation. They do **not** prove that a provider will choose the component or that the view looks correct in Pluto.

Human check: request the router view in the installed app. Confirm the agent emits it, the shape is recognizable at the answer card's width, dragging and part selection work, prose remains visible, and the old whiteboard/assembly answers still open. Check both themes. Record failures separately as routing, contract, rendering, accuracy, or performance failures.

The first scene deliberately uses projected SVG boxes. It does not claim curved geometry, AI-streamed detail, GPU rendering, or engineering accuracy. Add one of those capabilities only when a real prompt demonstrates a useful improvement. Before replacing a legacy visual, compare the same prompts side by side and retain the legacy fallback until the new view is at least as useful.
