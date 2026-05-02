# FEAT-049 — Multi-root `local_prefix` for Expo Router and similar layouts

- **Status**: design approved, plan pending
- **Source**: GitHub issue [#16](https://github.com/parisgroup-ai/graphify/issues/16)
- **Date**: 2026-05-02

## Problem

`graphify.toml`'s `[[project]].local_prefix` accepts a single string. The walker
uses it both as a "namespace wrapper" (file paths get the prefix prepended when
they don't already start with it) and as a hint for the resolver and the
`graphify suggest stubs` shadow-set.

This works for projects with a single source root (`src/`, `app/`, `crate::`)
but breaks down for projects with multiple parallel top-level directories that
have no common parent. The motivating case is Expo Router apps such as
`apps/mobile` in the ToStudy monorepo, where source lives under `app/`,
`lib/`, `components/`, `constants/`, and `i18n/` — none of which share a
prefix and none of which sit under a `src/` folder.

Each existing workaround degrades the analysis:

- **Omit `local_prefix`**: bare top-level dirs become global namespaces
  (`lib.foo`, `components.Button`) that collide with sibling projects' names
  in `graphify-summary.json`. Observed symptom on a 16-project monorepo:
  `web → mobile: 2197 edges (21 shared modules)` where all 21 modules are
  third-party packages, not actual cross-project coupling.
- **`local_prefix = "app"`**: the walker prepends `app.` to every file outside
  `app/` (e.g. `lib/util.ts` → `app.lib.util`). Imports in code are written
  as `@/lib/util` and resolve to `lib.util` in the resolver, so the prepended
  IDs miss `known_modules` and get classified external.
- **Restrict `source_dirs` to one root**: drops 60–70% of code from analysis.
- **Refactor the project to a single `src/`**: out of scope for this issue.

## Goals

1. Let users declare multiple parallel local roots in a single `[[project]]`
   block.
2. Preserve current behavior of every existing config (string form keeps
   wrapping; baselines and `graphify diff` snapshots stay valid).
3. Cut the false-positive cross-project edge count introduced by Expo-shaped
   projects without forcing a project rename or restructure.

## Non-goals

- Wrapping module IDs by project `name` (e.g. `mobile.lib.foo`). The user
  explicitly rejected this in brainstorming Q1; out of scope.
- Auto-detecting multi-root projects and returning a `Vec` from
  `detect_local_prefix`. Out of scope per Q4 — multi-root is opt-in via
  config; auto-detect emits an advisory warning when the pattern is suspected
  but does not switch behavior on its own.
- Changing `consolidation.allowlist` semantics. The allowlist matches the
  leaf symbol name and is agnostic to the prefix shape; no code path needs
  adjustment.
- Multi-root for PHP. PSR-4 in `composer.json` already provides per-namespace
  root mappings; setting `local_prefix` on PHP projects is documented as
  ignored today (DOC-002). Multi-root array on PHP is rejected fail-fast.

## Configuration design

### TOML surface

```toml
# Existing form — wrapping mode preserved.
[[project]]
name = "graphify-core"
repo = "./crates/graphify-core"
lang = ["rust"]
local_prefix = "src"

# New form — array, no-wrap mode.
[[project]]
name = "mobile"
repo = "./apps/mobile"
lang = ["typescript"]
local_prefix = ["app", "lib", "components", "constants", "i18n"]
```

### Rust types

The config field becomes a `serde(untagged)` enum so both shapes deserialize
into the same field name:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocalPrefix {
    /// Single string, current behavior — files outside the prefix get it
    /// prepended to their module ID.
    Single(String),
    /// Array of top-level dirs treated as local roots. Files keep their
    /// natural path-as-module-ID; the array drives is-local hints,
    /// shadow-set, and barrel-exclusion candidates without modifying IDs.
    Multi(Vec<String>),
}

pub struct Project {
    // ...
    pub local_prefix: Option<LocalPrefix>,
    // ...
}
```

To keep the rest of the codebase simple, `load_config` collapses the parsed
form into an internal struct after validation:

```rust
pub struct EffectiveLocalPrefix {
    /// One element when `Single`, N when `Multi`. Always non-empty.
    pub prefixes: Vec<String>,
    /// `true` for `Single` (apply wrapping). `false` for `Multi`
    /// (no wrapping, paths stand as-is).
    pub wrap: bool,
}
```

Every downstream site that today accepts `&str` accepts
`&EffectiveLocalPrefix` instead. An empty `prefixes` vector is unrepresentable
by construction.

### Validation rules (`load_config`)

- `Multi(v)` with `v.is_empty()` → fail-fast with the offending project name.
- `Multi(v)` with `v.len() == 1` → emit a stderr warning suggesting the
  string form (skips wrapping silently otherwise, which would surprise
  authors migrating from `local_prefix = "src"` to `["src"]`).
- `Multi(v)` containing duplicates → silent dedup, preserving first-seen
  order.
- `Multi(_)` on a PHP-only project (`lang = ["php"]`) → fail-fast with a
  clear message pointing at PSR-4 in `composer.json`.
- `Single("")` → treated as omitted (matches current behavior).

## Walker semantics

### Discovery

Unchanged. The walker still discovers every file under `repo` filtered by
language and excludes, and marks every discovered file `is_local = true`.
The array form does not narrow discovery — `is_local` classification stays
"discovered = local," which keeps a single mental model for the walker.

### `path_to_module`

```rust
pub fn path_to_module(
    base: &Path,
    file: &Path,
    prefix: &EffectiveLocalPrefix,
) -> String {
    // Compute the natural module id from the relative path (existing logic):
    //   stripped extension, '/' → '.', __init__/index collapse to parent.
    let module = compute_natural_module(base, file);

    if !prefix.wrap {
        // Multi mode: no wrapping. Path stands as-is.
        return module;
    }

    // Single mode: existing behavior — prepend the prefix unless already there.
    let p = &prefix.prefixes[0];
    if p.is_empty() || module.starts_with(p) {
        module
    } else {
        format!("{}.{}", p, module)
    }
}
```

`path_to_go_package` and `discover_files_with_psr4` mirror this — receive
`&EffectiveLocalPrefix`, branch on `wrap`. PHP path is unaffected because
`Multi` is rejected upstream.

## Resolver impact

`ModuleResolver::set_local_prefix(prefix: &str)` becomes
`set_local_prefix(prefixes: &[String])`. Internal `local_prefix: String` field
becomes `local_prefixes: Vec<String>`. The `apply_local_prefix(id)` helper
keeps its call sites but its semantics depend on a `wrap_mode` flag stored on
the resolver:

- `wrap_mode = true` (`Single`): existing behavior — prepend `prefixes[0]` if
  the id doesn't already start with it.
- `wrap_mode = false` (`Multi`): no-op — the id is already in canonical form.

Case 8.5 (`{from_module}.{raw}`) and case 8.6 (`{from_module}.{scoped}`) and
PHP case 7 are unaffected: they use `from_module`, which already carries the
final natural id (whether wrapped or not).

`ModuleResolver::is_local_module(id)` (used by the TS re-export walker)
stays unchanged — it still answers based on `known_modules`/`is_package`
membership. Multi-root mode does not need to consult the prefix list here,
because every discovered file is registered in `known_modules` with its
natural id (which already includes the top-level dir as its first segment
in no-wrap mode).

## Cache invalidation

`ExtractionCache` keys on the prefix string. With `Multi`, the cache key
becomes a deterministic join of the prefixes:

```rust
fn cache_key(prefix: &EffectiveLocalPrefix) -> String {
    if prefix.wrap {
        prefix.prefixes[0].clone()
    } else {
        // Multi mode marker + sorted prefixes joined with an unlikely separator.
        let mut sorted = prefix.prefixes.clone();
        sorted.sort();
        format!("multi:{}", sorted.join("|"))
    }
}
```

`multi:` prefix means a cache file written under `Single` mode never matches
a `Multi`-mode build with the same single dir name (e.g. `Single("app")` vs
`Multi(["app"])`) — different IDs would land in the graph (`app.foo` vs
`foo`), so cache invalidation is correct.

Reordering the array does not invalidate the cache (sort first).

## Barrel-exclusion (FEAT-028 cycle suppression) impact

`barrel_exclusion_ids` today returns `Some(local_prefix.into())` when the
project is in TypeScript and the consolidation allowlist matches. With
`Multi`:

```rust
fn barrel_exclusion_ids(
    project: &Project,
    consolidation: &ConsolidationConfig,
    graph: &CodeGraph,
) -> Vec<String> {
    let prefix = match &project.local_prefix {
        Some(LocalPrefix::Single(s)) if !s.is_empty() => vec![s.clone()],
        Some(LocalPrefix::Multi(v)) => v.clone(),
        _ => return vec![],
    };

    // Allowlist gating + intersection with actual graph nodes.
    prefix
        .into_iter()
        .filter(|id| consolidation.allowlist_matches_leaf(id))
        .filter(|id| graph.contains_node(id))
        .collect()
}
```

The opt-in (`suppress_barrel_cycles = true` + allowlist match) still gates
the feature; the change is that an array project can have multiple barrel
candidates and we suppress the ones that exist as graph nodes (each
top-level dir's `index.ts`/`mod.rs`).

## `graphify suggest stubs` shadow-set

`SuggestProject` field rename:

```rust
pub struct SuggestProject<'a> {
    // ...
    pub local_prefixes: &'a [&'a str],
    // ...
}
```

Shadow set construction inserts every prefix individually so none can be
suggested as an `external_stub`:

```rust
for p in project.local_prefixes {
    if !p.is_empty() {
        shadow_set.insert((*p).to_string());
    }
}
```

Top-segment-of-local-node logic is unchanged — already iterates all local
nodes regardless of prefix shape.

## Auto-detect

`detect_local_prefix(...)` keeps its `Option<String>` return type. A new
secondary heuristic emits a single-line warning when:

- Two or more top-level dirs each contain ≥10 source files for the project's
  configured languages, AND
- Top-1 file count < 3× top-2 file count (no clear winner).

Warning message:

```
[<project>] Multi-root pattern detected: candidates [app, lib, components, ...].
            Consider local_prefix = ["app", "lib", "components", ...] in graphify.toml.
            Auto-detected single prefix '<top1>' for now.
```

The detector still returns `top1`; behavior unchanged unless the user reads
stderr and acts.

## End-to-end behavior (Expo case)

Before (`local_prefix` omitted):

```
react.useState         → bare → also exists in apps/web → counted as "shared"
@trpc/client.useQuery  → bare → same
lib.api.client         → bare → also potentially exists in another project's lib/
```

After (`local_prefix = ["app", "lib", "components", "constants", "i18n"]`):

```
app.(tabs)._layout         is_local=true   (top segment "app" recognized)
lib.api.client             is_local=true   (top segment "lib" recognized)
components.Button          is_local=true
constants.routes           is_local=true
i18n.pt-BR                 is_local=true
react.useState             is_local=false  (no match in known_modules; resolver
                                            classifies external; if `react` is in
                                            external_stubs, marked Expected.)
```

`graphify-summary.json` cross-project edge count drops because the resolver
no longer treats `react.*` and `@trpc/client.*` as belonging to mobile's
local space, and they aren't dual-classified as web's local space either.

## Compatibility

- **Existing configs**: zero change. `local_prefix = "src"` deserializes as
  `Single("src")` and follows the wrapping path verbatim.
- **Existing baselines**: stay valid. `graphify diff` against an old
  `analysis.json` works because IDs in single-prefix projects don't move.
- **Migration**: when an author flips a project from `Single` to `Multi`,
  IDs change and the cache is invalidated automatically. `graphify diff`
  against a pre-flip baseline will surface added/removed nodes — that's the
  correct signal, the migration is a structural change.

## Tests (sketch)

- `path_to_module_no_wrap_keeps_natural_id` — `lib/foo.ts` under
  `Multi(["app","lib"])` → `lib.foo`.
- `path_to_module_wrap_uses_single_prefix` — current behavior preserved.
- `path_to_module_wrap_idempotent_on_already_prefixed` — `src/foo.ts` under
  `Single("src")` → `src.foo` (not `src.src.foo`).
- `cache_invalidates_on_prefix_change` — `Single("src")` cache does not
  load under `Multi(["src"])` config.
- `cache_stable_under_array_reorder` — `Multi(["app","lib"])` and
  `Multi(["lib","app"])` share a cache.
- `resolver_resolves_lib_imports_in_multi_root` — `@/lib/foo` →
  `lib.foo` resolves to a known local module.
- `single_element_array_emits_warning` — `Multi(["src"])` triggers the
  stderr advisory.
- `empty_array_fails_fast` — `Multi([])` aborts `load_config` with the
  project name in the error.
- `php_rejects_multi_prefix` — `Multi` on `lang = ["php"]` aborts.
- `barrel_exclusion_array_mode_uses_intersection` — exclusion list contains
  only prefixes whose top-level node exists.
- `auto_detect_multi_root_warning_triggered` — heuristic fires when top-1 <
  3× top-2 with both ≥10 files.
- `auto_detect_multi_root_warning_silent_under_threshold` — does not fire
  when top-1 dominates.
- `suggest_stubs_skips_all_multi_prefixes` — none of `app|lib|components`
  is suggested as an `external_stub`.

## Risks and follow-ups

- **`is_local_module` precision in array mode**: the helper currently uses
  `known_modules` membership. With multi-root, an import that resolves to a
  top-level dir name (e.g. `lib`) without a corresponding module file (no
  `lib/index.ts`) will not be in `known_modules` and will be classified
  external. This is a pre-existing limitation, not new — flagging in case
  Expo projects without barrel index files surface it.
- **Single-element `Multi` semantics**: warned, not rejected, because there
  is a legitimate (rare) case where an author wants no-wrap with a single
  root. The warning is friction enough.
- **Heuristic false positives in auto-detect warning**: the 3× ratio
  threshold is conservative but not bulletproof. The warning is advisory
  only, no behavior change — acceptable.
- **Future**: if cross-project namespace collisions remain a real pain even
  after FEAT-049, a separate `project_namespace` field (Q1 option B) is the
  natural follow-up. Out of scope here.
