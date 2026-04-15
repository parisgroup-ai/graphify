Run Graphify architectural analysis on the current project (full pipeline + summary).

## Instructions

1. Check `graphify.toml` exists. If not, run `graphify init` and help the user configure it.
2. Run the full pipeline: `graphify run --config graphify.toml`
3. Read `architecture_report.md` from the output directory and summarize:
   - Top 5 hotspots (score + hotspot_kind + rationale)
   - Circular dependencies (with break-candidate edge per cycle)
   - Community clusters (named by dominant concern)
   - Cross-project dependencies (if multi-project)
4. If the user provided a question in `$ARGUMENTS`, answer it using `analysis.json`.

## Arguments

`$ARGUMENTS` — Optional: specific module or question to investigate. If empty, give a full overview.
