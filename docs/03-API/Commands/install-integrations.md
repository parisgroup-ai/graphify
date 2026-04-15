# `graphify install-integrations`

Installs Graphify's AI-assistant integrations (agents, skills, slash commands, MCP server) into the user's Claude Code and Codex directories.

For the end-to-end team flow (solo vs. shared repo vs. CI), see [[AI Integrations]]. This page is the CLI reference.

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

## What gets installed

- **Agents:** `graphify-analyst` (Opus, MCP-preferred), `graphify-ci-guardian` (Haiku, CLI-only)
- **Skills:** `graphify-onboarding`, `graphify-refactor-plan`, `graphify-drift-check`
- **Slash commands:** `/gf-setup`, `/gf-analyze`, `/gf-onboard`, `/gf-refactor-plan`, `/gf-drift-check`
- **MCP server:** `graphify` entry merged into `~/.claude.json` (or `./.mcp.json`) and/or `~/.codex/config.toml`

After install, run `/gf-setup` from inside the client to verify and re-run future upgrades.

## Manifest

A `.graphify-install.json` file is written to each install root. It records every file written, its sha256, and MCP config paths touched. `--uninstall` uses this to remove only what `install-integrations` produced — user-authored customizations survive.

## Exit codes

- `0` — success (or dry-run preview)
- `1` — no client detected, I/O error, or malformed config

## Related

- [[AI Integrations]] — setup flow for solo + team
- [[MCP Server]] — `graphify-mcp` binary reference
