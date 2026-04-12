# FEAT-001: Interactive HTML Graph Visualization — Design

**Task:** [[FEAT-001-interactive-html-visualization]]
**Status:** Approved
**Date:** 2026-04-12
**Priority:** High
**Estimate:** 16h

## Overview

Self-contained HTML file that renders Graphify's dependency graph as an interactive force-directed visualization. Zero runtime dependencies — one file, open it anywhere, it just works.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Graph layout | Force-directed (D3 force simulation) | Classic, self-organizing, proven for dependency graphs |
| Rendering | SVG/Canvas auto-switch at 300 nodes | SVG for crisp interaction below threshold, Canvas for performance above |
| Interactivity | Full explorer | Community collapse, drag, edge type toggles, force sliders, PNG export |
| Asset embedding | Inline everything | Matches Graphify's standalone philosophy; ~300-400KB total |
| Cycle treatment | Animated marching ants + sidebar list | High visual impact, sidebar provides drill-down navigation |

## File Structure

```
crates/graphify-report/
├── src/
│   ├── lib.rs          # add: pub mod html; pub use html::write_html;
│   ├── html.rs         # Assembles HTML string, injects data
│   ├── json.rs
│   ├── csv.rs
│   └── markdown.rs
└── assets/
    ├── d3.v7.min.js    # D3.js v7 minified (~260KB), include_str!'d
    ├── graph.js         # Visualization code (~400-600 lines)
    └── graph.css        # Explorer UI styles (~150 lines)
```

## Data Flow

```
write_html(project_name, graph, metrics, communities, cycles, path)
    │
    ├── Serialize graph + analysis into a single JSON blob
    │   (same data as graph.json + analysis.json, merged)
    │
    ├── Build HTML string:
    │   ├── <!DOCTYPE html> + <head> with embedded <style> (from graph.css)
    │   ├── <script> with D3.js (from d3.v7.min.js)
    │   ├── <script>const GRAPHIFY_DATA = {merged JSON}</script>
    │   ├── <body> with sidebar + canvas/SVG container
    │   └── <script> with graph.js (visualization logic)
    │
    └── Write to architecture_graph.html
```

## Function Signature

```rust
pub fn write_html(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    path: &Path,
)
```

Same inputs as other report writers. No new Cargo dependencies — uses `serde_json` (already a workspace dep) and `include_str!` for asset embedding.

## HTML Layout

```
┌─────────────────────────────────────────────────────────┐
│  Graphify: {project_name}                    [📷 PNG]   │  Header bar
├──────────────┬──────────────────────────────────────────┤
│              │                                          │
│  SIDEBAR     │           GRAPH VIEWPORT                 │
│  (280px)     │           (flex-grow)                    │
│              │                                          │
│  Summary     │     Force-directed graph                 │
│  Filters     │     SVG (<= 300 nodes)                  │
│  Communities │     Canvas (> 300 nodes)                 │
│  Cycles      │                                          │
│  Force ⚙️    │                                          │
│  Search      │                                          │
│              │                                          │
├──────────────┴──────────────────────────────────────────┤
│  Tooltip: module details on hover                       │  Footer
└─────────────────────────────────────────────────────────┘
```

### Sidebar Sections

1. **Summary** — Node count, edge count, community count, cycle count.
2. **Filters** — Checkboxes: language (Python/TS), edge types (Imports/Defines/Calls).
3. **Communities** — Collapsible list. Click header to highlight community nodes. Click to collapse/expand the community into a single group node.
4. **Cycles** — Clickable list. Clicking zooms to cycle, dims non-participants, animates marching ants.
5. **Force controls** — Sliders for charge, link distance, center gravity.
6. **Search** — Text input with debounced substring match (case-insensitive) on node `id`.

### Header

- Project name as title.
- PNG export button.

### Footer

- Persistent tooltip area showing hovered node details (id, kind, score, betweenness, pagerank, in_degree, out_degree, community, in_cycle).

## Rendering Engine

### SVG Mode (nodes <= 300)

- D3 force simulation positions nodes.
- Nodes: SVG `<circle>`, radius proportional to `score` (min 4px, max 20px).
- Edges: SVG `<line>`, opacity proportional to `weight`.
- Native SVG events for hover/click/drag.
- Community colors from D3 `schemeSet3` (12-color categorical).

### Canvas Mode (nodes > 300)

- Same D3 force simulation.
- Drawn to `<canvas>` each tick via `requestAnimationFrame`.
- Hit-testing via quadtree for mousemove/click/drag.
- Community collapse/expand recomputes simulation with group nodes.

### Auto-Switch

```javascript
const CANVAS_THRESHOLD = 300;
const renderer = filteredNodes.length > CANVAS_THRESHOLD ? 'canvas' : 'svg';
```

Re-evaluated on each filter change. Switching to SVG when filtered below threshold.

### Node Sizing

```
radius = MIN_R + (node.score / maxScore) * (MAX_R - MIN_R)
// MIN_R = 4, MAX_R = 20
```

### Edge Rendering

| Edge kind | Style | Color |
|-----------|-------|-------|
| Imports | Solid line | `#666` (neutral) |
| Defines | Dashed line | `#2196F3` (blue) |
| Calls | Dotted line | `#4CAF50` (green) |
| In cycle | Solid, thicker (3px) | `#F44336` (red) |

Edge opacity: `0.3 + 0.7 * (weight / maxWeight)`.

## Interactivity

### Zoom and Pan

- D3 zoom behavior on viewport container.
- Scroll to zoom, drag background to pan.
- Double-click to reset view (fit-all).

### Node Interaction

- **Hover**: Footer tooltip updates with full node metrics.
- **Click**: Highlights node + direct neighbors, dims everything else to 10% opacity. Click background to clear.
- **Drag**: Repositions node, pins it on release. Double-click pinned node to unpin.

### Community Collapse/Expand

- Click community header in sidebar toggles.
- **Collapsed**: All community nodes replaced by single group node labeled `C{id} (N)`. External edges aggregated (weight summed).
- **Expanded**: Individual nodes restored, simulation reheated.

### Cycle Highlighting

1. Click cycle in sidebar.
2. Zoom to cycle's bounding box with padding.
3. Non-participating nodes/edges dimmed to 10% opacity.
4. Cycle edges: red, 3px thick.
5. **Marching ants**: `stroke-dasharray` animation (SVG) or frame-by-frame dash offset (Canvas). Flows in dependency direction (A→B→C→A).
6. Click background or another cycle to clear.

### Search

- Debounced text input (200ms).
- Matches against node `id` (substring, case-insensitive).
- Matching nodes pulse with highlight ring.
- Single match auto-centers viewport.
- Non-matching nodes dim to 30% opacity while active.

### PNG Export

- SVG mode: serialize SVG → offscreen canvas → `toDataURL('image/png')` → download.
- Canvas mode: `canvas.toDataURL('image/png')` directly.
- Filename: `{project_name}-graph.png`.

### Force Parameter Sliders

| Slider | Controls | Default | Range |
|--------|----------|---------|-------|
| Charge | `forceManyBody().strength()` | -120 | -300 to -10 |
| Link distance | `forceLink().distance()` | 80 | 20 to 300 |
| Center gravity | `forceCenter` strength | 0.05 | 0 to 0.3 |

Changes reheat simulation: `simulation.alpha(0.3).restart()`.

## CLI and Config Integration

### Config

`"html"` recognized as a format in `graphify.toml`:

```toml
[settings]
format = ["json", "csv", "md", "html"]
```

### CLI

New match arm in `write_all_outputs` in `main.rs`:

```rust
"html" => {
    write_html(
        project_name,
        graph,
        metrics,
        communities,
        cycles,
        &out_dir.join("architecture_graph.html"),
    );
}
```

### Init Template

Update commented format list:

```toml
# format = ["json", "csv", "md", "html"]    # output formats
```

### Output

```
report/my-project/
├── graph.json
├── analysis.json
├── graph_nodes.csv
├── graph_edges.csv
├── architecture_report.md
└── architecture_graph.html    ← NEW
```

## Testing

### Unit Tests (html.rs)

1. **`write_html_creates_file`** — Small graph, file exists and non-empty.
2. **`html_contains_data_block`** — Output contains `GRAPHIFY_DATA` with valid JSON.
3. **`html_contains_d3`** — Output contains D3 fingerprint (`d3.forceSimulation`).
4. **`html_contains_project_name`** — Project name in `<title>` and header.
5. **`html_empty_graph`** — 0 nodes renders gracefully.
6. **`html_single_node_no_edges`** — 1 node, 0 edges renders without error.

### Integration Test

- **`full_pipeline_html_output`** — Full pipeline with `format = ["html"]`, verifies file created with expected structures.

### Manual QA

- Open HTML in browser, verify force layout.
- Test each sidebar section.
- Test SVG mode (small graph) and Canvas mode (large graph).
- Test PNG export.
- Test marching ants on cycles.
- Test community collapse/expand.
- Open file offline to confirm self-containment.
