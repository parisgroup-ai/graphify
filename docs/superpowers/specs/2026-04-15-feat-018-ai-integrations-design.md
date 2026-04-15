# FEAT-018 — AI Integrations (Skills, Agents & `install-integrations`)

**Status:** Design approved (2026-04-15)
**Task:** FEAT-018 (to be created in TaskNotes at plan time)
**Related:** FEAT-007 (MCP server), FEAT-015 (`pr-summary`), FEAT-002 (drift detection), FEAT-017 (hotspot classification)

---

## 1. Scope

Ship a first-class set of AI-assistant integrations that make Graphify the architectural-analysis layer for Claude Code and Codex workflows. Integrations live inside the Graphify repository (`integrations/`) and are installed into the user's AI-client directories via a new `graphify install-integrations` subcommand.

The integrations target three recurring workflows — codebase onboarding, refactor planning, and architectural drift gating — and expose a polyvalent analyst agent for ad-hoc queries plus a deterministic CI-gate agent.

### In scope (v1)

- Two Claude Code agents under `integrations/claude-code/agents/`:
  - `graphify-analyst` — polyvalent, MCP-preferred, Opus
  - `graphify-ci-guardian` — deterministic CI gate, CLI-only, Haiku
- Three Claude Code skills under `integrations/claude-code/skills/`:
  - `graphify-onboarding`
  - `graphify-refactor-plan`
  - `graphify-drift-check`
- Four slash commands under `integrations/claude-code/commands/`, all prefixed `gf-`:
  - `gf-analyze`, `gf-onboard`, `gf-refactor-plan`, `gf-drift-check`
- MCP registration templates for both clients under `integrations/mcp/`
- Codex parity via automatic bridging (existing `~/.codex/claude-agent-bridge/sync.sh` when present; inline fallback otherwise)
- New CLI subcommand `graphify install-integrations` with install, dry-run, force, uninstall, and project-local modes
- Install manifest (`.graphify-install.json`) for safe uninstall and upgrades
- Integration tests for the install flow + snapshot tests for artifact contents

### Out of scope (v1) — rejected alternatives

| Rejected | Reason |
|---|---|
| Dedicated `graphify-pr-review` skill | Different workflow shape; deserves its own spec. `code-consolidation` Phase 5 and `graphify pr-summary` already cover the review gap |
| Per-issue specialized agents (hotspot-fix, cycle-fix, explain-module) | Same prompt shape as the polyvalent analyst; discovery cost outweighs specialization benefit. Folded into S2 (`graphify-refactor-plan`) + ad-hoc A1 usage |
| Six workflow skills (onboarding, refactor, hotspot, cycle, drift, explain) | Same prompt shape across four of them; collapsed into S2. `explain-module` is the analyst's default behavior, not a skill |
| Single agent for both interactive and CI modes | Conversational prompt vs. deterministic gate compromise each other; separate prompts + separate model tiers (Opus vs. Haiku) is cheaper and clearer |
| Shell script instead of CLI subcommand | Not discoverable via `graphify --help`; no manifest, no uninstall, no tests |
| Artifacts hosted in `parisgroup-ai/ai-skills-parisgroup` | Couples public/personal Graphify repo to an internal team repo; breaks CLI↔skill version lock |
| MCP required for all agents | Forces MCP setup in CI runners; overkill for A2 |
| MCP required for the polyvalent agent (no CLI fallback) | Blocks users without MCP registered; A1 already has enough CLI surface to degrade gracefully |
| Skills named without `graphify-` prefix | Collides with unrelated integrations (`frontend-design:onboarding`, etc.) in auto-match |
| GitHub Action template shipping in this PR | Operational concern; can ride a follow-up PR |
| Watch-mode auto-invocation of skills | Changes Graphify's runtime model; deserves its own spec |
| Telemetry of skill invocations | Product question; not an infrastructure concern |

---

## 2. Architecture overview

### 2.1 Artifact catalogue

| Kind | Name | Invocation | Model | Graphify access |
|---|---|---|---|---|
| Agent | `graphify-analyst` | Delegated by skills / ad-hoc | Opus | MCP preferred, CLI fallback |
| Agent | `graphify-ci-guardian` | Delegated by S3 (always) | Haiku | CLI only |
| Skill | `graphify-onboarding` | Auto-trigger / `/gf-onboard` | n/a (orchestrator) | Bash |
| Skill | `graphify-refactor-plan` | Auto-trigger / `/gf-refactor-plan` | n/a | Bash |
| Skill | `graphify-drift-check` | Auto-trigger / `/gf-drift-check` / CI | n/a | Bash |
| Command | `/gf-analyze` | User slash command | n/a | Bash (reuses existing `analyze.md`) |
| Command | `/gf-onboard` | User slash command | n/a | Thin wrapper invoking S1 |
| Command | `/gf-refactor-plan` | User slash command | n/a | Thin wrapper invoking S2 |
| Command | `/gf-drift-check` | User slash command | n/a | Thin wrapper invoking S3 |

### 2.2 Source-of-truth layout in the Graphify repo

```
graphify/
├── integrations/
│   ├── README.md                            # Audience, conventions, contribution guide
│   ├── claude-code/
│   │   ├── agents/
│   │   │   ├── graphify-analyst.md
│   │   │   └── graphify-ci-guardian.md
│   │   ├── skills/
│   │   │   ├── graphify-onboarding/SKILL.md
│   │   │   ├── graphify-refactor-plan/SKILL.md
│   │   │   └── graphify-drift-check/SKILL.md
│   │   └── commands/
│   │       ├── gf-analyze.md
│   │       ├── gf-onboard.md
│   │       ├── gf-refactor-plan.md
│   │       └── gf-drift-check.md
│   ├── codex/
│   │   └── prompts/                         # Codex-flavored command wrappers
│   │       ├── gf-analyze.md
│   │       ├── gf-onboard.md
│   │       ├── gf-refactor-plan.md
│   │       └── gf-drift-check.md
│   └── mcp/
│       ├── claude-code.json                 # Template merged into ~/.claude.json (or ./.mcp.json)
│       └── codex.toml                       # Template merged into ~/.codex/config.toml
└── crates/graphify-cli/src/commands/
    └── install_integrations.rs              # New subcommand
```

### 2.3 Runtime flow

```
User → slash command (e.g. /gf-onboard)
       ↓
       Skill (S1/S2/S3) — orchestrator running Bash + prerequisite checks
       ↓
       Invokes A1 graphify-analyst (Task tool / subagent)
       ↓
       A1 queries graph via MCP (or CLI fallback)
       ↓
       Returns structured analysis → skill renders final output file / chat summary

CI pipeline → graphify run + graphify check
           ↓
           Skill S3 (or plain CLI) invokes A2 graphify-ci-guardian
           ↓
           A2 interprets check-report.json + drift-report.json
           ↓
           Markdown to stdout, warnings to stderr, deterministic exit code
```

A1 is never the user's primary contact point in the common path; skills own the user-facing surface. The user *may* invoke A1 directly for ad-hoc analysis, but that is exception, not rule. This keeps auto-invocation triggers at the skill layer (rich descriptions) and lets the agent focus on *doing the work* rather than interpreting intent.

### 2.4 Installation flow

```
graphify install-integrations --claude-code --codex [--project-local] [--skip-mcp] [--dry-run] [--force] [--uninstall]
  │
  ├─ Auto-detect targets (flags override)
  │    Claude Code:   ~/.claude/ exists (or ./.claude when --project-local)
  │    Codex:         ~/.agents/skills/ exists (always global; --project-local is ignored for Codex)
  │
  ├─ For each enabled target:
  │    a. Copy agents   → <target>/agents/
  │    b. Copy skills   → <target>/skills/
  │    c. Copy commands → <target>/commands/ (Claude Code) or <target>/prompts/ (Codex)
  │    d. If --codex and ~/.codex/claude-agent-bridge/sync.sh exists: run bridge
  │       Else: write inline wrappers to ~/.agents/skills/claude-agent-graphify-*/SKILL.md
  │
  ├─ Unless --skip-mcp:
  │    a. Merge integrations/mcp/claude-code.json → ~/.claude.json (or ./.mcp.json)
  │    b. Merge integrations/mcp/codex.toml       → ~/.codex/config.toml
  │    c. Binary path: `which graphify-mcp` → fallback to env::current_exe().with_file_name("graphify-mcp")
  │
  ├─ Write manifest: <install-root>/.graphify-install.json
  │    { graphify_version, installed_at, files: [ {path, sha256, kind} ] }
  │
  └─ Print summary: N agents, M skills, K commands installed + next steps
```

Guarantees:
- **Idempotent** — running twice produces no changes when sources are unchanged
- **Non-destructive by default** — existing files with different content are skipped with a warning; `--force` overwrites
- **Safe uninstall** — `--uninstall` removes only manifest-tracked files; user customizations survive
- **MCP merge preserves other entries** — never overwrites unrelated MCP servers in the user's config

---

## 3. Agent designs

### 3.1 `graphify-analyst` (A1)

**Frontmatter:**

```yaml
---
name: graphify-analyst
description: "Investigates dependency graphs to answer architectural questions. Explains hotspots (hub/bridge/mixed), traces cycles, maps communities, suggests refactor targets. Use when a skill needs deep graph analysis or when the user asks 'why is X a hotspot', 'what depends on Y', 'what's coupled with Z'."
model: opus
tools:
  - mcp__graphify__graphify_stats
  - mcp__graphify__graphify_search
  - mcp__graphify__graphify_explain
  - mcp__graphify__graphify_path
  - mcp__graphify__graphify_all_paths
  - mcp__graphify__graphify_dependents
  - mcp__graphify__graphify_dependencies
  - mcp__graphify__graphify_suggest
  - mcp__graphify__graphify_transitive_dependents
  - Bash
  - Read
  - Grep
  - Glob
min_graphify_version: "0.6.0"
---
```

**System prompt covers:**

1. **Mode detection** — on its first tool call in a given conversation, the agent attempts an MCP tool (typically `graphify_stats`); on MCP error it switches to CLI mode for the rest of that conversation and logs the switch to stderr
2. **Metrics interpretation** — translate betweenness, PageRank, in-degree, in-cycle, and the FEAT-017 `hotspot_kind` (`hub` / `bridge` / `mixed`) into plain language
3. **Query patterns** — canonical flows for common requests:
   - "Explain module X" → `graphify_explain(node_id=X)` + fan-in/out sanity check via `graphify_dependents` / `graphify_dependencies`
   - "Why is X a hotspot" → explain metric breakdown + show which scoring weight dominates
   - "Trace path A → B" → `graphify_path`, then `graphify_all_paths` with `max_depth=10, max_paths=5`
   - "Find candidate to break cycle C" → sort cycle edges by `weight × confidence` ASC; the lowest-product edge is the safest break candidate (few call sites × uncertain extraction = low disruption if removed)
4. **Refactor suggestions** — for a hotspot, recommend a split axis based on dependency partitioning visible in `graphify_dependencies` (separates modules that only import I/O from those that only import logic, etc.)
5. **Output contract with skills** — when invoked by S1/S2, return Markdown with stable section headers the skill can parse or embed verbatim

**Non-responsibilities:**

- Does not modify source code (analytical, not implementer)
- Does not run tests
- Does not write git commits
- Does not invoke other agents (prevents recursion blow-ups)

### 3.2 `graphify-ci-guardian` (A2)

**Frontmatter:**

```yaml
---
name: graphify-ci-guardian
description: "Gates CI on architectural drift. Runs graphify check + diff against a baseline, produces a markdown PR comment and a deterministic exit code. Use in CI workflows and pre-merge hooks; DO NOT use for interactive exploration."
model: haiku
tools:
  - Bash
  - Read
min_graphify_version: "0.6.0"
---
```

**System prompt covers:**

1. **Input contract** — caller provides paths to `check-report.json` and optionally `drift-report.json`; baseline is already resolved upstream
2. **Output contract:**
   - stdout: a Markdown block matching the `graphify pr-summary` style (suitable for `gh pr comment --body-file -`)
   - stderr: warnings, non-fatal errors
   - exit 0: no new violations, no hotspot regressed past threshold
   - exit 1: any new cycle, any hotspot over the configured threshold, or any contract violation surfaced by `graphify check`
3. **Determinism rules:**
   - No hedging language ("might be concerning", "looks risky")
   - No refactor suggestions (leave those to A1 when invoked by the user)
   - Fixed section order: Status → New Violations → Hotspot Regressions → Improvements → Footer
   - Cite exact numbers (score, delta, threshold); never summarize as adjectives
4. **Error handling** — if `check-report.json` missing, exit 1 with a stderr message; never emit an "OK" on missing inputs

**Why this is an agent rather than a shell script:**

- Skill S3 can hand it partial inputs (only `check-report.json`, no drift) and expect a valid render
- Output adapts to what's present (omit empty sections) without a templating system
- A future enhancement (e.g., "summarize N PRs' drift in a weekly digest") reuses the same agent

### 3.3 Codex parity for agents

Both agents are authored in Claude Code format. Bridging to Codex happens in `install-integrations`:

1. Copy `.md` files to `~/.claude/agents/` (unchanged)
2. If `~/.codex/claude-agent-bridge/sync.sh` is present: invoke it; it generates `~/.agents/skills/claude-agent-graphify-<name>/SKILL.md` wrappers that reference the Claude agent source
3. Otherwise: write inline wrappers directly to `~/.agents/skills/claude-agent-graphify-<name>/SKILL.md`, embedding the full agent body. This mirrors the bridge's output format and keeps the Codex invocation path stable (`claude-agent-graphify-analyst`, etc.)

The inline fallback is what makes Codex parity **actually** automatic — users in clean environments don't need to hunt down the bridge script.

---

## 4. Skill designs

All three skills share a common contract for prerequisite checks and analyst invocation:

- Verify `graphify` is on `PATH`; otherwise emit install guidance and abort
- Verify `graphify.toml` exists; otherwise run `graphify init` interactively
- Ensure a recent analysis exists (mtime of `report/<project>/analysis.json` less than 7 days old); otherwise run `graphify run`
- When delegating to A1, pass a self-contained prompt including: project name, analysis path(s), the exact question, and the required output format

### 4.1 `graphify-onboarding` (S1)

**Frontmatter:**

```yaml
---
name: graphify-onboarding
description: "Produce an architecture tour of a codebase using Graphify. Use when a user joins a new project, asks to 'explain the architecture', 'map the codebase', 'give me an overview', or 'onboard me to this repo'. Long-form read-once output covering communities, hotspots (hub/bridge/mixed), cycles, and cross-project coupling."
version: 1.0.0
min_graphify_version: "0.6.0"
---
```

**Flow:**

1. Run prerequisite checks (see common contract above)
2. Read `report/<project>/analysis.json` and `report/graphify-summary.json` (if multi-project)
3. Invoke A1 with: *"Produce an architecture tour for `<project>`. Include sections: Snapshot (node/edge/community/cycle counts), Communities (named by dominant concern, anchored at highest-score node), Top Hotspots (top 5 by score, with FEAT-017 classification and one-line rationale), Cycles (ranked by risk = size × max-node-score, show break candidate per cycle), Cross-project Coupling (only if multi-project; edges with `confidence < 0.7`), Recommended Actions (top 3, ordered). Use the exact section headers listed."*
4. Write A1's output to `docs/architecture/graphify-tour-<YYYY-MM-DD>.md` (configurable via skill argument)
5. Emit a 1-paragraph chat summary + link to the file

**Output structure (enforced by A1 via the prompt):**

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

### 4.2 `graphify-refactor-plan` (S2)

**Frontmatter:**

```yaml
---
name: graphify-refactor-plan
description: "Generate a prioritized, multi-phase architectural refactor plan from Graphify analysis. Use when the user says 'plan a refactor', 'where should I start refactoring', 'reduce coupling', or wants to tackle hotspots, cycles, or consolidation systematically."
version: 1.0.0
min_graphify_version: "0.6.0"
---
```

**Flow:**

1. Run prerequisite checks
2. (Optional) if `report/baseline/analysis.json` exists, load it for drift-aware prioritization (candidates that got worse since baseline rise in the ranking)
3. List all cycles and top 5 hotspots by score
4. For each item, invoke A1 with: *"Minimum-disruption fix for `<issue>`? Include: target edge/node to modify, estimated effort (1 of: `file-move`, `api-rename`, `signature-change`, `split`, `consolidate`), expected score delta, verification command. Do not suggest implementation code."*
5. Consolidate into ranked plan:
   - **Phase 1 — Break Cycles** (all cycles; bloquers for any other refactor)
   - **Phase 2 — Hotspots** (ordered: `hub` → split; `bridge` → reduce fan-in; `mixed` → deeper investigation first)
   - **Phase 3 — Consolidation** (delegates to the `code-consolidation` skill when available; otherwise leaves a structured placeholder with the data needed)
   - **Phase 4 — Verification** (`graphify diff` + `graphify check` against pre-refactor snapshot)
6. Write to `docs/plans/refactor-plan-<YYYY-MM-DD>.md`

**Output structure** mirrors the format above, with one row per item and a summary header giving total estimated PRs and expected cumulative score delta (both A1-estimated; not ground truth).

### 4.3 `graphify-drift-check` (S3)

**Frontmatter:**

```yaml
---
name: graphify-drift-check
description: "Run architectural drift gate. Compares the current Graphify analysis against a baseline and fails on regression (new cycles, hotspot growth, threshold breach). Use in CI, pre-merge hooks, and when the user asks to 'check drift', 'gate this PR', or 'verify no regression'."
version: 1.0.0
min_graphify_version: "0.6.0"
---
```

**Flow:**

1. Detect mode: `CI` env var non-empty → non-interactive; otherwise interactive
2. Resolve baseline:
   - Skill argument `--baseline <path>` if provided
   - Else `report/baseline/analysis.json` if present
   - Else abort with clear instructions to produce one (`graphify run && cp report/<project>/analysis.json report/baseline/`)
3. Run `graphify run --force` (forces fresh extraction; cache bypass guarantees gate sees real current state)
4. Run `graphify check --json`; outputs `report/<project>/check-report.json`
5. Run `graphify diff --before <baseline> --after report/<project>/analysis.json`; outputs `drift-report.json` + `drift-report.md`
6. Invoke A2 with the two report paths; A2 produces Markdown + exit code
7. Propagate A2's exit code as the skill's exit status
8. In interactive mode, append a one-liner suggesting `/gf-refactor-plan` if violations exist

**CI output** (A2-controlled):

```markdown
## Graphify Drift Report

**Status:** 🔴 1 new violation

### New Cycles (1)
- `app.auth → app.db → app.auth` (confidence: 0.9, weight: 3)

### Hotspot Regressions (1)
- `app.services.llm`: 0.78 → 0.91 (+0.13, threshold 0.85)

### Improvements
- `app.utils.format`: 0.62 → 0.41

---
*Exit: 1 · graphify 0.6.0*
```

---

## 5. Slash commands

Each command is a 10–30 line wrapper that invokes the corresponding skill with sensible defaults. Commands are the user-facing entry point; skills own orchestration.

### 5.1 Catalogue

| Command | Purpose | Default behavior |
|---|---|---|
| `/gf-analyze` | Reuses existing `analyze.md` logic | Runs `graphify run` + summarizes top findings |
| `/gf-onboard` | Invokes `graphify-onboarding` skill | Writes tour to `docs/architecture/` |
| `/gf-refactor-plan` | Invokes `graphify-refactor-plan` skill | Writes plan to `docs/plans/` |
| `/gf-drift-check` | Invokes `graphify-drift-check` skill | Compares current vs. `report/baseline/` |

### 5.2 Why prefix with `gf-`

- Commands are typed repeatedly; short prefix reduces friction
- Prefixed commands cluster visually in `/` autocomplete
- Avoids collision with unrelated tools' slash commands (`/analyze`, `/drift`, etc.)
- Skills and agents keep the longer `graphify-` prefix because they are identifiers in auto-match discovery where uniqueness matters more than typing speed

### 5.3 Codex version

Codex prompts live under `integrations/codex/prompts/` and differ only in tool references (e.g., `Bash` → `shell` where applicable). Generated from the Claude versions via a small translation function in `install-integrations`.

---

## 6. `graphify install-integrations` subcommand

### 6.1 CLI surface

```
graphify install-integrations [OPTIONS]

OPTIONS:
  --claude-code            Install Claude Code artifacts (default: auto-detect)
  --codex                  Install Codex artifacts (default: auto-detect)
  --project-local          Install Claude Code artifacts to ./.claude/ instead of ~/.claude/ (Codex artifacts remain global; Codex has no native project-local skills mechanism)
  --skip-mcp               Do not register graphify-mcp in MCP configs
  --dry-run                Show what would be done without writing
  --force                  Overwrite existing files (default: skip + warn)
  --uninstall              Remove all manifest-tracked artifacts
  -h, --help               Print help
```

Auto-detection: `~/.claude/` present → enable `--claude-code`; `~/.agents/skills/` present → enable `--codex`. With no flags and neither detected, the subcommand exits 1 with guidance.

### 6.2 Manifest format

Written to `<install-root>/.graphify-install.json`:

```json
{
  "graphify_version": "0.7.0",
  "installed_at": "2026-04-15T10:20:30Z",
  "files": [
    {
      "path": "/Users/user/.claude/agents/graphify-analyst.md",
      "sha256": "abc123...",
      "kind": "agent"
    },
    {
      "path": "/Users/user/.claude/skills/graphify-onboarding/SKILL.md",
      "sha256": "def456...",
      "kind": "skill"
    }
  ],
  "mcp": {
    "claude_code": { "path": "/Users/user/.claude.json", "added_key": "graphify" },
    "codex": { "path": "/Users/user/.codex/config.toml", "added_section": "mcp_servers.graphify" }
  }
}
```

### 6.3 MCP merge semantics

Both Claude Code and Codex configs are merged, not overwritten:

- **Claude Code** (`~/.claude.json` or `./.mcp.json`): parse existing JSON, add/update `mcpServers.graphify`, preserve all other keys, write back with stable key ordering
- **Codex** (`~/.codex/config.toml`): parse existing TOML, add/update `[mcp_servers.graphify]`, preserve other sections, write back

The `graphify` MCP server entry points at `which graphify-mcp` or, when that fails, at `env::current_exe().with_file_name("graphify-mcp")`. Manifest records which config path was touched so uninstall can remove only that one key.

### 6.4 Uninstall semantics

`--uninstall`:
1. Read manifest
2. For each listed file: if sha256 matches current contents, delete; otherwise warn ("user-modified, skipping") and leave in place
3. For MCP entries: parse config, remove only the `graphify` key/section, write back preserving everything else
4. Delete manifest last (only after successful cleanup)

This design guarantees that **uninstalling never deletes user-authored customizations**, even when the user edited an installed file in place.

---

## 7. Testing

### 7.1 Unit tests (`graphify-cli/src/commands/install_integrations.rs`)

- YAML frontmatter parsing for agents/skills (validate all artifacts in `integrations/` parse cleanly at build time)
- MCP config merge: JSON merge with preserved sibling keys; TOML merge with preserved sibling sections
- Manifest round-trip: write → read → equality
- Conflict detection: existing destination with different sha → reported as conflict; identical sha → no-op

### 7.2 Integration tests

Using a temp-directory `$HOME` stub and a fabricated source `integrations/` tree:

- Install to empty target → all files present, manifest correct
- Install twice with no changes → no writes on second run
- Install with `--force` over modified files → overwrites + manifest updated
- `--dry-run` → no writes, exit 0, prints plan
- `--uninstall` → only manifest-tracked files removed; hand-authored sibling files survive
- `--project-local` → writes to `./.claude/` and `./.codex/`; optionally updates `.gitignore`
- `--skip-mcp` → no changes to `~/.claude.json` / `~/.codex/config.toml`

### 7.3 Snapshot tests (CI)

A test locks content hashes of every file under `integrations/` into `integrations/.manifest.lock.json`. Changes to any file require regenerating the lock, which is reviewed explicitly at PR time. Prevents accidental drift between integration sources and what ships.

### 7.4 Manual QA checklist (release)

- Fresh install on macOS and Linux
- `/gf-onboard` against `apps/ana-service` (or equivalent Python project) produces a coherent tour
- `/gf-drift-check` with a real baseline produces correct exit code and PR-comment Markdown
- Ad-hoc invocation of `graphify-analyst` via `Task` tool (Claude Code) and via `claude-agent-graphify-analyst` skill (Codex) both work
- `--uninstall` leaves user customizations intact

---

## 8. Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Version skew (skill uses a CLI flag the installed binary lacks) | Runtime failure in skill | Skills declare `min_graphify_version` in frontmatter; `install-integrations` warns when upgrading onto an older binary |
| Codex bridge script absent | Codex parity silently breaks | Inline fallback writes Codex wrappers directly; no dependency on the external bridge |
| MCP not registered in the user's client | A1 falls back to CLI (still functions) | A1 detects on boot and logs; `install-integrations` prints a recipe if `--skip-mcp` was used |
| Claude Code or Codex changes skill/agent format upstream | Artifacts silently obsolete | CI runs `install-integrations` in a clean container and smoke-tests an invocation per PR |
| Name collision with user's existing `graphify-analyst.md` | Overwrites hand-authored work | Default is non-destructive; `--force` required; diff is displayed before the skip |
| Project-local install pollutes git | `.claude/skills/*` appears in commits | `install-integrations --project-local` updates `.gitignore` if absent; warns if the user already tracks `.claude/` explicitly |
| MCP config has pre-existing `graphify` entry from a different source | Overwrites foreign config | Merge logic checks for a `_graphify_managed` flag in the entry; only overwrites self-managed entries; logs a conflict otherwise |

---

## 9. Versioning & compatibility

- Artifacts inherit the workspace version (`workspace.package.version` in root `Cargo.toml`)
- Breaking changes to skills/agents bump MINOR and require a changelog entry
- Manifest records `graphify_version` at install time; installing over an old manifest runs a cleanup-then-install cycle
- `min_graphify_version` in each skill's frontmatter prevents runtime surprises when the user's binary lags

---

## 10. Decisions log

Notable choices made during design, with the rationale preserved here so future readers don't relitigate:

1. **Hybrid agent shape** (1 polyvalent + 1 specialized, rather than pure single or pure specialized) — keeps discovery cost low while allowing the CI gate to run on a cheaper model with a deterministic prompt
2. **Skills as thin orchestrators** — all heavy lifting lives in A1; skills are ~150–300 lines of prerequisite checks + agent invocation + output rendering. Fixing a bug in the analysis logic fixes it everywhere
3. **`gf-` prefix on commands, `graphify-` prefix on skills/agents** — commands optimize for typing, identifiers optimize for uniqueness in auto-match
4. **MCP preferred but not required** — A1 works in both worlds; A2 doesn't need MCP at all
5. **Install subcommand, not shell script** — discoverable via `--help`, testable, versioned, supports clean uninstall
6. **Source of truth in `graphify/integrations/`** — couples skill evolution to CLI evolution; skills can rely on CLI features available in the binary that shipped them
7. **Manifest-driven uninstall** — guarantees user customizations survive; similar to package-manager conventions

---

## 11. Explicitly deferred

Items that make sense but belong in follow-up specs:

- `graphify-pr-review` skill (PR review as a dedicated workflow)
- GitHub Action / GitLab template for drift-check in CI
- Auto-invocation of skills triggered by `graphify watch` events
- Telemetry on skill invocation counts and latencies
- Web dashboard consuming `graphify-mcp`

---

## 12. Acceptance criteria

This feature is done when:

1. All four Claude Code command files, three skill files, and two agent files exist under `integrations/` with frontmatter that parses cleanly
2. `integrations/mcp/` contains valid config templates for both clients
3. `graphify install-integrations` installs artifacts to the correct directories, respects `--dry-run` / `--force` / `--uninstall` / `--project-local` / `--skip-mcp`, and writes a valid manifest
4. Running `install-integrations` twice in a row is a no-op on the second run
5. Uninstall never removes user-authored files or MCP entries it did not create
6. `/gf-onboard`, `/gf-refactor-plan`, and `/gf-drift-check` each produce their documented output on a real project
7. Codex parity: the same three workflows work via `claude-agent-graphify-*` skills, tested manually on one real invocation each
8. Unit + integration + snapshot tests all pass in CI
9. The existing test count grows to reflect new coverage; no prior tests regress
10. CLI `--help` shows the new subcommand; README and CLAUDE.md are updated with the user story
