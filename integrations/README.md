# Graphify Integrations

Source-of-truth for all Graphify AI-assistant artifacts (Claude Code + Codex).

## Layout

- `claude-code/agents/` — subagent definitions (`.md` with YAML frontmatter)
- `claude-code/skills/<name>/SKILL.md` — skill orchestrators
- `claude-code/commands/` — slash commands (prefix `gf-`)
- `codex/prompts/` — Codex-flavored slash prompts
- `mcp/claude-code.json` — MCP server template (Claude clients)
- `mcp/codex.toml` — MCP server template (Codex)

## Installation

End-users install these into their AI-client directories via:

```
graphify install-integrations
```

See `docs/superpowers/specs/2026-04-15-feat-018-ai-integrations-design.md` for the full design.

## Editing artifacts

- Every artifact must have valid YAML frontmatter parseable by `graphify-cli::install::frontmatter`
- Agents require `name`, `description`, `model`, `tools`, `min_graphify_version`
- Skills require `name`, `description`, `version`, `min_graphify_version`
- Commands have no required frontmatter fields but conventionally include `description`
- After editing, regenerate `integrations/.manifest.lock.json` by running `UPDATE_LOCK=1 cargo test -p graphify-cli --test install_integrations manifest_lock_is_up_to_date`
