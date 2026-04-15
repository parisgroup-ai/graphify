Auto-configure Graphify for this Codex install: verify the binary, ensure a `graphify.toml` exists, install skills + agent bridge + the MCP server, and report what still needs a manual step.

## Instructions

Run the checks in order. After each step, report the outcome in one short line before moving on.

### 1. Verify the `graphify` binary is on PATH

Run `graphify --version`.

- If it fails with "command not found":
  1. If you are inside the Graphify repo (there is a `Cargo.toml` with `[workspace]` listing `graphify-cli`), suggest `cargo install --path crates/graphify-cli --locked` and `cargo install --path crates/graphify-mcp --locked`.
  2. Otherwise, point the user to the GitHub releases page and hand them the exact command to run for their platform. Do NOT run installers on their behalf.
- If it succeeds, capture the version string for the final report.

Also check `graphify-mcp --help` (silence stdout — exit code is enough). If missing, warn that MCP tools will not work until it is on PATH.

### 2. Ensure `graphify.toml` exists in the current project

Run `test -f graphify.toml` (or equivalent).

- If missing: run `graphify init`, then open `graphify.toml` and help the user set `local_prefix` and `[[project]].repo` to match the actual source layout. Ask before editing — show a proposed diff first.
- If present: read it and report the project count and languages.

### 3. Install integrations

Codex installs are always global (the `--project-local` flag is ignored on this client). Run:

```
graphify install-integrations --codex
```

Re-install after editing artifacts: append `--force`. Forward any flags from `$ARGUMENTS` verbatim (e.g., `--force`, `--uninstall`, `--skip-mcp`, `--dry-run`).

This will:
- Copy skills to `~/.agents/skills/`
- Copy Codex prompts to `~/.codex/prompts/`
- Install the Codex-bridge agent wrappers (inline fallback if the bridge script is missing)
- Merge the `graphify` entry into `~/.codex/config.toml` under `[mcp_servers.graphify]`

Parse the command output for:
- Number of files installed
- Conflict list — if non-empty, stop and ask the user whether to re-run with `--force`
- MCP registration path

### 4. Verify MCP registration

Read `~/.codex/config.toml` and confirm the `[mcp_servers.graphify]` section exists and `command` points to the `graphify-mcp` binary path. If the path does not exist on disk, flag it as broken and suggest re-running step 3.

### 5. Reload notice

Tell the user they must **restart Codex** before the MCP tools become callable. Newly installed slash prompts like `/gf-analyze` are picked up on the next prompt without a restart.

### 6. Final report

Output a compact block:

```
graphify:     <version>          (bin: <path>)
graphify-mcp: <version or "MISSING">
config:       <path>              (<N> projects, langs: <list>)
installed:    <M> files → ~/.agents + ~/.codex
mcp:          registered in ~/.codex/config.toml
next:         /gf-onboard  or  /gf-analyze
```

End with one sentence telling the user the single most important next action.

## Arguments

`$ARGUMENTS` — Optional flags forwarded to `graphify install-integrations`: `--force`, `--uninstall`, `--skip-mcp`, `--dry-run`.
