---
uid: bug-014
status: done
priority: low
scheduled: 2026-04-14
completed: 2026-04-15
pomodoros: 0
contexts:
- diff
- trend
- dx
tags:
- task
- bug
---

# `graphify diff --before history/*.json` fails with cryptic schema error

## Description

When a user points `graphify diff --before` at a file in `report/{project}/history/`, the command fails with:

```
Invalid analysis JSON "report/pkg-api/history/1776203987090556000.json": missing field `betweenness` at line 109 column 5
```

The underlying reason is correct — `history/*.json` snapshots are in the lightweight **trend-format** (used by `graphify trend`) and are missing fields the full analysis schema requires (`betweenness`, `pagerank`, and likely others). But the error message doesn't explain that, leaving the user to assume either the file is corrupt or `diff` is broken.

## Reproduction

```bash
graphify run                                    # populates history/*.json
graphify diff \
  --before report/pkg-api/history/<snapshot>.json \
  --after  report/pkg-api/analysis.json
# → Invalid analysis JSON: missing field `betweenness` at line 109 column 5
```

Observed on `graphify v0.6.0`.

## Impact

- Users trying to compare "before this session's refactor" vs "after" reach for history snapshots intuitively (they exist, they're timestamped, they're in `report/`).
- Current workaround requires knowing to `cp report/{project}/analysis.json report/{project}/baseline.json` *before* starting a refactor — but there's no hint telling you to do that until you hit this error.
- The error message points at a JSON structural problem, nudging users down a wrong diagnostic path (file corruption, version mismatch).

## Proposed Fix (pick one)

**Option A — clearer error (minimal change, low risk).** Detect the schema shape. If the file matches the trend/history format, return:

```
Error: <path> is a trend-format history snapshot, not a full analysis.

History snapshots are only consumable by `graphify trend`. To diff, use a full
`analysis.json`. If you need a pre-session baseline, copy the current analysis
before the refactor:
  cp report/{project}/analysis.json report/{project}/baseline.json
```

**Option B — upgrade history to full schema (more work, best UX).** Make `graphify run` persist the full analysis schema into `history/`. `trend` ignores the extra fields (it already reads a subset). `diff` Just Works against any history snapshot. Cost: disk space — the full schema is ~4.5× larger on the pkg-api example (3.3 MB vs ~700 KB). Could mitigate with zstd or field-filtered compression.

**Option C — auto-fallback (compromise).** `diff` detects trend format and, if possible, re-extracts the missing fields via `--config`. Likely not feasible because betweenness requires the full graph, not just the node list.

Recommendation: start with A. Revisit B if drift detection across sessions becomes a common workflow.

## Likely Scope (Option A)

- Add schema-detection helper: "is this file a full analysis or a trend snapshot?"
- Change the error path in `diff` (`graphify-cli` or `graphify-core::diff`) to branch on that helper.
- Mention history vs analysis distinction in the `graphify diff --help` text.
- Add a small CLI test that asserts the new error message on a known history fixture.

## Subtasks

- [x] Add `is_trend_snapshot_json(text: &str) -> bool` helper in `graphify-core::history` (parses minimal `{captured_at, project}` discriminator; zero cost on happy path — only called on deserialize failure).
- [x] Branch the error path in `diff` (`graphify-cli/src/main.rs::load_snapshot`) to return the explanatory message when the helper matches; generic error retained for other parse failures.
- [x] Update `graphify diff --help` long description to note the `analysis.json` requirement and the baseline-copy recipe.
- [x] Add CLI test (`crates/graphify-cli/tests/diff_error_messages.rs`) asserting the new message on a trend fixture and asserting malformed JSON is not misclassified.
- [ ] (Optional) Mention the baseline-copy recipe in README under "Drift detection across sessions" — deferred; the in-error message now carries the recipe, which is where users hit it.

## Notes

- Workaround documented for users today: `cp report/{project}/analysis.json report/{project}/baseline.json` before the refactor, then `graphify diff --before baseline.json --after report/{project}/analysis.json`.
- The CLAUDE.md in one consumer project (ToStudy) currently advertises `graphify diff --baseline report/analysis.json --config graphify.toml` — that works (baseline-vs-live mode re-extracts), but reinforces the assumption that the baseline exists from the last run, which it doesn't unless explicitly preserved.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-014-historical-architecture-trend-tracking]] — history file producer
- [[FEAT-002-architectural-drift-detection]] — diff consumer
