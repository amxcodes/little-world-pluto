# Pluto v0.1.17

- Fixes explicitly requested 3D character visualizations being converted into
  text before they reach the model viewer.
- Preserves the existing text fallback for character scenes when a visual is
  not requested or the user asks for text only.
- Adds regression coverage for explicit, unrequested, and text-only character
  visualization requests.

This release does not change the 3D planning, authoring, validation, or rendering
pipeline.
