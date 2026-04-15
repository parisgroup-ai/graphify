---
title: "graphify init"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
related:
  - "[[CLI Reference]]"
  - "[[Configuration]]"
---

# `graphify init`

Generate a starter `graphify.toml` template in the current directory.

## Synopsis

```bash
graphify init
```

## Arguments

None.

## Flags

None.

## Behavior

Writes `graphify.toml` to the current directory with:

- A commented `[settings]` block (output, weights, exclude, format)
- A single `[[project]]` block (`name = "my-project"`, `repo = "."`, `lang = ["python"]`, etc.)
- Inline comments explaining each field

Edit the file before running other commands. See [[Configuration]] for the full reference.

## Examples

```bash
# Create a starter config in the current directory
graphify init

# Then edit graphify.toml and run the pipeline
$EDITOR graphify.toml
graphify run
```

## Output

| Target | Content |
|---|---|
| `./graphify.toml` | Starter config file |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | File written |
| 1 | Write failed (permissions, disk full) |

## Gotchas

> [!warning] Overwrites silently
> If `graphify.toml` already exists in the current directory, `init` **overwrites** it without prompting. `git stash` your customizations first.

## Related

- [[Configuration]] — full reference for the file you just generated
- [[First Steps]] — what to do after `init`
- [[CLI Reference]]
