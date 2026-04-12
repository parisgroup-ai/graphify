Run Graphify architectural analysis on the current project.

## Instructions

1. Check if `graphify.toml` exists in the project root. If not, run `graphify init` and help the user configure it.

2. Run the full pipeline:
```bash
graphify run --config graphify.toml
```

3. Read the generated `architecture_report.md` from the output directory.

4. Summarize the key findings:
   - **Top 5 hotspots** — modules with highest coupling score (betweenness + pagerank + in-degree + in-cycle)
   - **Circular dependencies** — list each cycle and suggest which edge to break
   - **Community clusters** — group related modules and name each cluster by its dominant concern
   - **Cross-project dependencies** — if multi-project, highlight coupling between projects

5. If the user provided a specific question (e.g., "is module X too coupled?"), answer it using the analysis data in `analysis.json`.

## Arguments

$ARGUMENTS — Optional: specific module or question to investigate. If empty, give a full architectural overview.
