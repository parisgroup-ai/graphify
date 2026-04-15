# `graphify install-integrations`

Installs Graphify's AI-assistant integrations (agents, skills, slash commands, MCP server) into the user's Claude Code and Codex directories.

## Usage

```
graphify install-integrations [OPTIONS]
```

## Options

| Flag | Meaning |
|---|---|
| `--claude-code` | Target Claude Code (default: auto-detect `~/.claude/`) |
| `--codex` | Target Codex (default: auto-detect `~/.agents/skills/`) |
| `--project-local` | Install Claude Code artifacts to `./.claude/` (Codex stays global) |
| `--skip-mcp` | Do not merge the `graphify` MCP server into client configs |
| `--dry-run` | Show what would be done without writing |
| `--force` | Overwrite user-modified files |
| `--uninstall` | Remove manifest-tracked artifacts and MCP entries |

## Manifest

A `.graphify-install.json` file is written to each install root. It records every file written, its sha256, and MCP config paths touched. `--uninstall` uses this to remove only what `install-integrations` produced — user-authored customizations survive.

## Exit codes

- `0` — success (or dry-run preview)
- `1` — no client detected, I/O error, or malformed config
