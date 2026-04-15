# gf-setup

Auto-configure Graphify for this Claude Code install: verify the binary, ensure a `graphify.toml` exists, install agents/skills/commands + the MCP server, and report what still needs a manual step.

## Instructions

Run the checks in order. After each step, report the outcome in one short line before moving on.

### 1. Verify the `graphify` binary is on PATH

Run `graphify --version`.

- If it fails with "command not found":
  1. Check the repo context. If you are inside the Graphify repo (there is a `Cargo.toml` with `[workspace]` listing `graphify-cli`), suggest `cargo install --path crates/graphify-cli --locked` and `cargo install --path crates/graphify-mcp --locked`.
  2. Otherwise, point the user to the GitHub releases page and ask which platform binary they want. Do NOT download or execute installers on their behalf — hand them the exact command to run.
- If it succeeds, capture the version string for the final report.

Also check `graphify-mcp --help` (silence stdout — exit code is enough). If missing, warn that MCP tools will not work until it is on PATH in the same directory as `graphify`.

### 2. Ensure `graphify.toml` exists in the current project

Run `test -f graphify.toml` (or equivalent).

- If missing: run `graphify init`, then open `graphify.toml` and help the user set `local_prefix` and `[[project]].repo` to match the actual source layout. Ask before editing — show a proposed diff first.
- If present: read it and report the project count and languages.

### 3. Install integrations

Pick flags based on intent (ask the user if ambiguous):

- Project-local (recommended when the user wants this repo's teammates to share the same skills):
  `graphify install-integrations --project-local`
- User-global (default for solo use):
  `graphify install-integrations`
- Re-install after editing artifacts:
  `graphify install-integrations --force`

Forward any flags from `$ARGUMENTS` verbatim (e.g., `--project-local`, `--force`, `--uninstall`, `--skip-mcp`, `--dry-run`).

Parse the command output for:
- Number of files installed
- Conflict list (files that already exist with a different hash) — if non-empty, stop and ask the user whether to re-run with `--force`
- MCP registration paths

### 4. Verify MCP registration

Depending on scope:
- Global: read `~/.claude.json` and confirm `mcpServers.graphify` exists and points to the `graphify-mcp` binary path.
- Project-local: read `./.mcp.json` at the repo root.

If the binary path in the config does not exist on disk, flag it as broken and suggest re-running step 3.

### 5. Reload notice

Tell the user they must **restart Claude Code** (or run `/mcp reload` if available) before the MCP tools (`graphify_query`, `graphify_path`, etc.) become callable. Newly installed slash commands like `/gf-analyze` are picked up on the next prompt without a restart.

### 6. Final report

Output a compact block:

```
graphify:     <version>          (bin: <path>)
graphify-mcp: <version or "MISSING">
config:       <path>              (<N> projects, langs: <list>)
installed:    <M> files → <install root>
mcp:          registered in <mcp config path>
next:         /gf-onboard  or  /gf-analyze
```

End with one sentence telling the user the single most important next action.

## Arguments

`$ARGUMENTS` — Optional flags forwarded to `graphify install-integrations`: `--project-local`, `--force`, `--uninstall`, `--skip-mcp`, `--dry-run`.
