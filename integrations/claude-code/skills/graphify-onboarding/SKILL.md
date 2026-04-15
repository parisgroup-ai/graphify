---
name: graphify-onboarding
description: "Produce an architecture tour of a codebase using Graphify. Use when a user joins a new project, asks to 'explain the architecture', 'map the codebase', 'give me an overview', or 'onboard me to this repo'. Long-form read-once output covering communities, hotspots (hub/bridge/mixed), cycles, and cross-project coupling."
version: 1.0.0
min_graphify_version: "0.6.0"
---

# Graphify Onboarding

## Purpose

Produce an architecture tour of a codebase for someone ramping up. Long-form, read-once, structured — intended to be committed to `docs/architecture/` so future newcomers can skim it.

## Prerequisites

```bash
if ! command -v graphify >/dev/null; then
  echo "graphify not installed. See https://github.com/cleitonparis/graphify" >&2
  exit 1
fi

if [ ! -f graphify.toml ]; then
  echo "graphify.toml not found in cwd. Run 'graphify init' to generate one." >&2
  exit 1
fi
```

Ensure a recent analysis exists (mtime of `report/<project>/analysis.json` less than 7 days old); otherwise run `graphify run --config graphify.toml`.

## Flow

1. Verify prerequisites (above)
2. Locate the primary analysis: `report/<project>/analysis.json`
   - If multi-project, also read `report/graphify-summary.json`
3. Delegate to the `graphify-analyst` agent via Task tool with this prompt:

   > Produce an architecture tour for `<project>`. Read `<analysis.json path>`. Include these sections in this exact order, with these exact headers:
   >
   > - `## Snapshot` — node count, edge count, community count, cycle count, languages
   > - `## Communities` — list communities named by dominant concern (top-score node anchors each), bullet count of modules per community
   > - `## Top Hotspots` — top 5 by score, as a markdown table with columns: Module, Score, Kind, Why
   > - `## Cycles` — all cycles ranked by risk (size × max-node-score); for each, show the cycle plus nominated break edge (lowest `weight × confidence`)
   > - `## Cross-project Coupling` — only if multi-project; list edges with `confidence < 0.7`
   > - `## Recommended Actions` — top 3, ordered
   >
   > Do not add preamble. Start with `## Snapshot`.

4. Write the agent's response to `docs/architecture/graphify-tour-$(date +%Y-%m-%d).md`, prepending `# Architecture Tour — <project>\n\n`
5. Report a 1-paragraph chat summary + a link to the file

## Output File Structure

```markdown
# Architecture Tour — <project>

## Snapshot
…

## Communities
…

## Top Hotspots
…

## Cycles
…

## Cross-project Coupling
…

## Recommended Actions
…
```
