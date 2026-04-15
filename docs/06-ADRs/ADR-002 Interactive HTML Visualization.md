---
title: "ADR-002: Interactive HTML Visualization"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-001"
tags:
  - type/adr
  - status/accepted
  - report
supersedes:
superseded_by:
---

# ADR-002: Interactive HTML Visualization

## Status

**Accepted** — 2026-04-12

## Context

Static Markdown reports surface hotspots and cycles as text and tables. They miss the spatial intuition that a force-directed graph gives — clusters are immediately visible, hubs are obvious, and exploration is interactive. We needed a visualization that:

- Works **offline**, with no server/runtime
- Runs in any browser without install steps
- Embeds in PR descriptions, wikis, and shared folders
- Matches Graphify's "standalone binary" philosophy on the consumer side

## Decision

**Chosen option:** Generate a **single self-contained HTML file** (`architecture_graph.html`) per project, with **D3.js v7** embedded via `include_str!`, plus inline CSS and a single JSON data block. Auto-switches between SVG (≤300 nodes) and Canvas (>300 nodes) for performance.

## Consequences

### Positive

- ~300–400KB per file; opens in any browser, online or offline
- Full explorer UX: drag, zoom, pan, edge-type toggles, force sliders, search, PNG export, community collapse, marching-ants on cycles
- Zero runtime deps for consumers (no `npm install`, no static-site server)
- Pairs well with Markdown report — text for scan, HTML for explore

### Negative

- D3 is bundled in every report (~260KB duplicated across projects)
- HTML is regenerated from scratch each run (no incremental render)
- Hard to diff visually in PRs (binary-ish artifact)
- Custom JS in `assets/graph.js` is `~600 LOC` of vanilla — no test coverage beyond smoke tests

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Self-contained HTML + D3** (chosen) | Offline, portable, rich UX | Big file, no incremental updates |
| Cytoscape.js | More layout algorithms | Larger bundle, similar trade-offs |
| External web app (separate repo) | Smaller artifacts | Requires hosting + auth |
| Static SVG only | Smallest | No interaction |
| GraphML/Cypher only | Best for power users | Excludes casual readers |

## Plan de Rollback

**Triggers:** Bundle size becomes a problem (e.g., monorepos with 50+ projects ship 15MB of duplicated D3) or a critical XSS/security concern in the bundled D3 version.

**Steps:**
1. Remove `"html"` from the default `[settings].format` list
2. Keep `crates/graphify-report/src/html.rs` available behind explicit opt-in
3. Optionally swap to a CDN-loaded D3 (re-introduces a network dependency)

**Validation:** `graphify run` no longer writes `architecture_graph.html` for projects that don't request it.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-interactive-html-visualization-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-interactive-html-visualization.md`
- Task: `[[FEAT-001-interactive-html-visualization]]`
- Related ADRs: [[ADR-001 Rust Rewrite]]
