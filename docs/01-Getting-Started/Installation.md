---
title: Installation
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/guide
  - getting-started
related:
  - "[[Configuration]]"
  - "[[First Steps]]"
  - "[[Troubleshooting]]"
---

# Installation

Graphify ships as a **standalone binary** (~3.5 MB) with no runtime dependencies. Targets macOS (Intel + ARM) and Linux (x86_64 + ARM, MUSL static).

## Option 1 — Pre-built binary (recommended)

Download the binary for your platform from the [GitHub Releases page](https://github.com/parisgroup-ai/graphify/releases) and place it on your `PATH`.

```bash
# macOS / Linux
chmod +x graphify
mv graphify /usr/local/bin/

# Verify
graphify --version
```

## Option 2 — Build from source

### Prerequisites

| Tool | Version |
|---|---|
| Rust toolchain | 2021 edition (stable) |
| `cargo` | bundled with rustup |
| C toolchain | required by `tree-sitter` (`cc`, `clang`, or MSVC) |

Install Rust if you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build

```bash
git clone https://github.com/parisgroup-ai/graphify.git
cd graphify
cargo build --release -p graphify-cli
```

The binary lands at `target/release/graphify`. Copy it anywhere on your `PATH`:

```bash
cp target/release/graphify /usr/local/bin/
```

### Verify

```bash
$ graphify --version
graphify 0.6.0
```

## Option 3 — Run from source (no install)

Useful while developing Graphify itself:

```bash
cargo run -p graphify-cli --release -- run --config graphify.toml
```

## MCP server

The MCP server is a separate binary in the same workspace:

```bash
cargo build --release -p graphify-mcp
# Binary at target/release/graphify-mcp
```

See [[🔍 Quick Reference#MCP server]] for usage.

## AI integrations (optional)

If you use Claude Code or Codex, install the bundled agents, skills, slash commands, and MCP server registration:

```bash
graphify install-integrations              # global: ~/.claude + ~/.agents
graphify install-integrations --project-local   # team-shared via ./.claude
```

Adds 5 slash commands (`/gf-setup`, `/gf-analyze`, `/gf-onboard`, `/gf-refactor-plan`, `/gf-drift-check`) plus live MCP tools. After install, restart your client and run `/gf-setup` to verify. Full guide: [[AI Integrations]].

## Running the test suite

```bash
cargo test --workspace            # full suite (all crates)
cargo test -p graphify-extract    # single crate
cargo test -p graphify-core
```

## Updating

```bash
git pull
cargo build --release -p graphify-cli
cp target/release/graphify /usr/local/bin/
```

## Uninstall

```bash
rm /usr/local/bin/graphify
rm /usr/local/bin/graphify-mcp   # if installed
```

Caches and reports are local to each project and live in:
- `<project_out>/.graphify-cache.json` — extraction cache (safe to delete)
- `<project_out>/` — generated reports (safe to delete)

## Next

- [[Configuration]] — write your `graphify.toml`
- [[First Steps]] — run your first analysis
- [[Troubleshooting]] — when something goes wrong
