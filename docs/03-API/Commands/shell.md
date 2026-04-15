---
title: "graphify shell"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - query
related:
  - "[[CLI Reference]]"
  - "[[query]]"
  - "[[explain]]"
  - "[[path]]"
---

# `graphify shell`

Interactive REPL for exploring the dependency graph. Same operations as `query` / `explain` / `path`, but with a frozen-graph context — the pipeline runs once at startup.

## Synopsis

```bash
graphify shell [--config <path>] [--project <name>]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--project <name>` | all | Load only the named project (default: load all) |

## Behavior

1. Run extract + analyze once at startup
2. Drop into a `graphify> ` prompt
3. Accept the same command verbs as the one-shot CLI
4. Hold the analyzed graph in memory until exit

## Built-in commands

```
graphify> query <pattern>           # glob-search nodes
graphify> path <source> <target>    # shortest path
graphify> explain <node_id>         # full profile
graphify> stats                     # per-project graph stats
graphify> help                      # show available commands
graphify> exit                      # quit
```

## Examples

```bash
# Single-project config
graphify shell

# Multi-project: load just one
graphify shell --project api

# Inside the REPL:
graphify> query "app.services.*"
graphify> explain app.services.llm
graphify> path app.main app.services.llm
graphify> stats
graphify> exit
```

## Output

Same human-readable formatting as the equivalent one-shot commands. **No `--json` mode** inside the shell.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Clean exit (`exit` / Ctrl+D) |
| 1 | Config error or fatal IO error during startup |

## Gotchas

- **Graph is frozen for the session.** Code edits during the shell session are not reflected. Restart to pick up changes.
- **No readline / history.** Plain `stdin` line reading — no up-arrow recall, no `Ctrl+R` search. Use a real terminal multiplexer (`tmux`, `screen`) or wrapper (`rlwrap graphify shell`) if you need them.
- **Invalid commands print a hint and return to the prompt** — never crash.
- **`stats` shows per-project breakdown** when multiple projects are loaded.

## See also

- [[query]] — same search, one-shot
- [[explain]] — same profile, one-shot
- [[path]] — same path search, one-shot
- [[ADR-004 Graph Query Interface]] — design rationale (no `rustyline` dep)
