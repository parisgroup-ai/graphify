---
uid: feat-009
status: done
completed: 2026-04-13
priority: low
timeEstimate: 720
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - report
  - export
---

# Additional export formats (Neo4j, GraphML, Obsidian, SVG)

Expand output beyond JSON/CSV/MD/HTML to support graph databases, desktop tools, and knowledge management.

## Goals

- **Neo4j Cypher** — generate `cypher.txt` with CREATE statements for bulk import
- **GraphML** — XML format compatible with Gephi, yEd, Cytoscape
- **Obsidian vault** — one .md file per community hub + index.md with wikilinks
- **SVG** — static vector export of the graph (embeddable in docs, GitHub)
- All formats configurable via `graphify.toml` `format` array

## Inspiration

safishamsi/graphify outputs to Neo4j Cypher, GraphML, SVG, and Obsidian vault. The Obsidian integration is especially interesting — it bridges graph analysis to knowledge management by generating navigable vaults with wikilinks from the report to community hub articles.

## Subtasks

- [x] Neo4j Cypher generator (CREATE nodes, MERGE edges, SET properties)
- [x] GraphML serializer (XML with node/edge attributes)
- [x] Obsidian vault generator (community hubs + index + wikilinks)
- [x] Add format variants to config parser and CLI
- [x] Tests for each format
- [x] Documentation update

## Follow-up

- SVG export remains a possible future extension, but was not part of the shipped scope recorded in `sprint.md`

## Notes

Each format is independently useful and can be implemented incrementally. GraphML is probably simplest (XML serialization). Neo4j Cypher is high value for teams using graph databases. Obsidian vault is unique and creative.

## Verification (2026-04-13)

- Verified CLI writes `graph.cypher`, `graph.graphml`, and `obsidian_vault`
- Verified report writers exist in `crates/graphify-report/src/neo4j.rs`, `graphml.rs`, and `obsidian.rs`
- Verified sprint history records implementation with 13 tests on 2026-04-13
