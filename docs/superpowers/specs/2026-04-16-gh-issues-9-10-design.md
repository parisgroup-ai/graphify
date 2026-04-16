# GitHub Issues 9 and 10 Design

## Goal

Fix two open extraction bugs:

1. TypeScript path aliases should resolve when `tsconfig.json` lives above the configured `repo` root or differs by subtree.
2. Go imports should resolve against package-scoped module IDs instead of file-scoped IDs.

## Scope

This change is intentionally narrow. It does not redesign the extraction pipeline or introduce external tooling like `go list`.

## Design

### TypeScript alias resolution

Current behavior loads a single `tsconfig.json` only from `<repo>/tsconfig.json`. That fails for common layouts such as `repo = "./src"` with `tsconfig.json` in the parent directory.

The fix is to support per-file TypeScript alias contexts:

- walk upward from each discovered TypeScript file to find the nearest `tsconfig.json`
- parse `compilerOptions.paths` for that config
- associate the alias set with the source file's module ID
- resolve alias imports using the source module's nearest config first

This preserves existing behavior for simple projects while fixing subtree and parent-root configs.

### Go package identity

Current behavior uses file-scoped IDs like `pkg.handler` for Go modules, but Go imports package paths, not file paths. That causes import edges to point at package-like nodes that never match the discovered file nodes.

The fix is to treat Go files as package nodes:

- discovered Go module IDs collapse to the containing directory instead of the file stem
- the project `local_prefix` remains the canonical package root prefix
- symbol IDs continue to nest under the package ID, so functions and methods remain distinct

This makes import edges and discovered local nodes share the same identity model.

## Testing

- add resolver tests for per-module TypeScript `tsconfig` lookup
- add CLI/integration-style coverage for `repo=./src` + parent `tsconfig.json`
- update walker tests to assert Go discovery uses package IDs
- update resolver tests to assert Go import resolution preserves the configured package root
