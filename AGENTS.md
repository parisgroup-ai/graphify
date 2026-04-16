# AGENTS.md

Root instructions for agents working in `parisgroup-ai/graphify`.

Parent context for Claude/Codex: `[[CLAUDE.md]]`

## Project Scope

Graphify is a Rust workspace for architecture analysis of codebases.

- `graphify-core`: graph types, metrics, communities, cycles, query engine
- `graphify-extract`: language extractors and module resolution
- `graphify-report`: JSON/CSV/Markdown/HTML/export renderers
- `graphify-cli`: `graphify` binary
- `graphify-mcp`: MCP server binary for AI assistants

Current workspace version: `0.8.2`

## Working Mode

This repository is currently operated in solo-dev mode.

- Work directly on `main`
- Do not require PRs for routine changes unless explicitly requested
- Keep commits intentional and scoped
- Preserve unrelated local files outside task scope, especially `.tasknotes.toml`

## Core Commands

```bash
cargo test --workspace
cargo fmt --all
cargo clippy --workspace -- -D warnings

cargo build --release -p graphify-cli -p graphify-mcp
cargo install --path crates/graphify-cli --force
cargo install --path crates/graphify-mcp --force
```

## Release Rules

Official release path for this repo is GitHub Releases driven by tags.

1. Update workspace version in root `Cargo.toml`
2. Update `CHANGELOG.md`
3. Keep `Cargo.lock` aligned when workspace package versions change
4. Push commit(s) to `main`
5. Create and push tag `vX.Y.Z`
6. GitHub Actions `Release` workflow publishes the binary tarballs

Verify release with:

```bash
gh run list --workflow Release --limit 3
gh release view vX.Y.Z
```

## Instruction Maintenance

- `CLAUDE.md` must link to `[[AGENTS.md]]`
- Prefer stable statements over fragile counts
- Update both instruction files when workflow or release process changes
