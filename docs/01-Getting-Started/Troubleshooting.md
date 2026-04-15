---
title: Troubleshooting
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/guide
  - getting-started
  - troubleshooting
related:
  - "[[Installation]]"
  - "[[Configuration]]"
  - "[[First Steps]]"
---

# Troubleshooting

Common issues, symptoms and fixes.

## Discovery / extraction

### Empty or near-empty graph

> [!warning] Symptom
> stderr shows `walker discovered 0 files` or `≤1 file` for a project.

**Likely cause:** `local_prefix` doesn't match your layout (or auto-detection picked the wrong root).

**Fix:**
1. Check the project layout. Where do your sources live? `src/`, `app/`, root, somewhere else?
2. Set `local_prefix` explicitly in `graphify.toml`:
   ```toml
   [[project]]
   name         = "api"
   repo         = "./apps/api"
   lang         = ["python"]
   local_prefix = "src"   # or "app", or your actual top-level package
   ```
3. Re-run with `--force` to bypass the cache.

### Test files included in the graph

Built-in excludes: `*.test.{ts,tsx,js,jsx}`, `*.spec.{ts,tsx,js,jsx}`, `*.test.py`, `*_test.py`. If you have non-standard naming (e.g., `test_*.py`):

```toml
[settings]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__"]
```

Add directories you want skipped. The `exclude` list is **directory-level**, not glob-level.

### TypeScript: external imports show up as local

> [!warning] Symptom
> Workspace packages like `@repo/logger` resolve to local node IDs with mangled paths.

**Cause:** `tsconfig.json` `paths` alias `@/*` is over-matching.

**Fix:** Verified in BUG-007/BUG-011 — current resolver only matches `@/*` when the target is inside the project. If you still see this, ensure your `tsconfig.json` `paths` are well-scoped (no `*` wildcards that catch scoped packages).

### Python: false circular dependencies

Fixed in BUG-001 — the resolver now uses `is_package` detection for `__init__.py` files. If you still see false cycles:

1. Confirm you're on a recent build (`graphify --version`).
2. Verify your `__init__.py` files are discovered (check `graph_nodes.csv` for them).
3. File a bug with a minimal repro.

## Cache

### Stale results after a code change

> [!info] Cause
> Cache key is SHA256 of file contents — pure changes invalidate, but config-level changes (e.g., a new `lang` entry) don't always.

**Fix:**
```bash
graphify run --config graphify.toml --force   # bypass cache
```

Or delete the cache:
```bash
rm <output>/<project>/.graphify-cache.json
```

> [!tip] Watch mode
> `--force` only applies to the **initial** rebuild in watch mode. Subsequent rebuilds always use the cache. Restart watch mode to force.

### Cache discarded silently

The cache is fully discarded (no error) when:
- Graphify version changes
- `local_prefix` changes for a project

This is intentional — both invalidate the entire extraction model.

## CLI exit codes

> [!warning] Convention
> All Graphify CLI errors use **exit 1** (uniform — not exit 2).

| Command | Exit 1 means |
|---|---|
| `graphify check` | Architecture violations found (cycles, hotspots, policy, contracts) |
| `graphify diff` | Required input missing or malformed |
| `graphify pr-summary` | Required input (`analysis.json`) missing |
| Any | Generic fatal error |

`graphify pr-summary` does **not** gate on findings — it always exits 0 unless a required input is missing. Gating is `graphify check`'s job.

## Watch mode

### High CPU on big repos

Watch debounces at 300ms but rebuilds the affected project. If a project has thousands of files, the per-project rebuild can be heavy.

**Mitigations:**
- Split the monorepo into more `[[project]]` blocks so each rebuild is smaller.
- Tighten `exclude` to skip generated output dirs.
- Consider running `graphify` in CI on `pre-push` instead of locally on every save.

### Watch ignores my edits

Watch only fires for files matching the included extensions and not in `exclude`. Edits to `.toml`, `.md`, `.json` etc. are ignored — by design.

## MCP server

### "Not found" or no response from AI assistant

The MCP server speaks JSON-RPC over **stdio**. Common gotchas:

1. **stdout pollution** — anything printed to stdout that isn't valid JSON-RPC corrupts the protocol. All Graphify diagnostics go to stderr; if you wrap the binary, ensure your wrapper doesn't print to stdout.
2. **Eager startup** — the server extracts on launch. For very large monorepos this can take seconds; the assistant may time out.
3. **Config required** — `graphify mcp --config graphify.toml` needs the same config that `run` would use.

## Reports

### `graphify-summary.json` missing

> [!info] By design
> Only generated when 2+ `[[project]]` blocks are configured.

### Stale project directories under `report/`

Fixed in BUG-013 — `graphify run` and `graphify report` now prune output directories for projects that no longer exist in the config (only when they contain only Graphify-generated artifacts).

If you have **other files** in those directories, Graphify will leave them alone. Move custom files elsewhere or delete the directory manually.

## Build from source

### `tree-sitter` build failure

Needs a C toolchain. macOS:
```bash
xcode-select --install
```

Linux:
```bash
sudo apt install build-essential   # Debian/Ubuntu
sudo dnf install gcc make          # Fedora
```

### Tests fail with file-system errors

The integration tests use `tempfile`. Make sure `/tmp` (or your `TMPDIR`) is writable and not full.

```bash
df -h /tmp
```

## Still stuck?

1. Re-run with `--force` to rule out cache.
2. Capture stderr — it's the source of truth for diagnostics.
3. Check the [[sprint|sprint board]] for known bugs.
4. Open an issue on GitHub with: command, config (sanitized), stderr, and `graphify --version`.
