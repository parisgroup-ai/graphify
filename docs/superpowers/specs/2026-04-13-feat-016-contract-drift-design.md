# FEAT-016 — Contract Drift Detection Between ORM and TypeScript Types

**Status:** Design approved (2026-04-13)
**Task:** [[FEAT-016-contract-drift-detection-between-orm-and-typescript]]
**Related:** FEAT-002 (drift detection), FEAT-004 (CI quality gates), FEAT-013 (policy rules), FEAT-005 (incremental cache)

---

## 1. Scope

Detect contract drift between a **Drizzle ORM schema** and a **frontend TypeScript interface/type** when both sides describe the same logical entity in a monorepo.

### In scope (v1)

- Drizzle ORM schemas (Postgres, MySQL, SQLite table builders)
- TypeScript `interface` and `type` alias declarations
- Scalar columns and full relation declarations, including cardinality
- Explicit pair declarations in `graphify.toml`
- Detection surfaces through `graphify check` (new gate alongside cycles, hotspots, policy)
- Built-in type map plus per-project overrides
- Automatic `snake_case` ↔ `camelCase` field-name normalization, plus explicit aliases

### Out of scope (v1)

- Prisma, TypeORM, Sequelize, or other ORMs (extension point reserved)
- Zod schemas, tRPC contracts, or OpenAPI documents as the TS side
- Convention-based auto-pairing (explicit config only)
- Drift-over-time history for contracts (this is a spot check, not a historical diff)
- Multi-project pair references (e.g., pair declared in project A with TS type in project B)
- Indexes, unique constraints, check constraints, defaults as comparison targets
- Neo4j, GraphML, and Obsidian output for contract findings (they describe graph structure, not drift)

### Detection classes

| Class | Description |
|---|---|
| `FieldMissingOnTs` | Scalar column exists in ORM, absent from the TS type |
| `FieldMissingOnOrm` | Scalar property exists in TS, absent from the ORM schema |
| `TypeMismatch` | Shared field with incompatible types under the effective type map |
| `NullabilityMismatch` | Shared field with differing null-acceptance |
| `RelationMissingOnTs` | Relation declared in ORM `relations()`, absent from TS |
| `RelationMissingOnOrm` | Relation present in TS, absent from ORM |
| `CardinalityMismatch` | Shared relation with differing `one`/`many` cardinality |
| `UnmappedOrmType` | ORM column type not in the effective type map (warning class) |

---

## 2. Architecture

The feature spans three existing crates along their current responsibility seams.

| Crate | Module | Role |
|---|---|---|
| `graphify-core` | `src/contract.rs` (new) | Normalized data model, pure comparison engine, violation types |
| `graphify-extract` | `src/drizzle.rs` (new) | Drizzle parser on top of the existing TS tree-sitter AST |
| `graphify-extract` | `src/ts_contract.rs` (new) | TS interface/type extractor on the same AST |
| `graphify-report` | `src/contract_json.rs` (new) | JSON output for `check.json` extension |
| `graphify-report` | `src/contract_markdown.rs` (new) | Markdown section for `architecture_report.md` |
| `graphify-cli` | `src/main.rs` | Config loading, wiring contracts into `graphify check` |

### Key architectural choices

- **No new tree-sitter grammar.** Drizzle schemas are TypeScript; the existing TS parser already produces the AST the Drizzle extractor walks.
- **Contract extraction runs alongside graph extraction.** Same file walk, same parser instance, same incremental cache. `Contract` values serialize next to `ExtractionResult`, so FEAT-005 covers contract data for free.
- **Pure comparison lives in core.** `graphify-core/src/contract.rs` has no IO dependency and no tree-sitter. Mirrors the split between `diff.rs` (pure) and `cache.rs` (IO) that the codebase already uses.
- **Comparison feeds the existing check pipeline.** The existing `CheckReport` is per-project (`projects[].violations[]` with a `CheckViolation` enum discriminated by `kind`). Contract violations span pairs that often cross projects, so the spec extends `CheckReport` with a new **workspace-level** `contracts` block alongside `projects[]`. This is an additive schema change: existing consumers that read `projects[].violations[]` stay unaffected. A single `graphify check` run evaluates per-project gates (cycles, hotspots, policy) as today and the workspace-level contract gate once.
- **Pair paths are workspace-root relative.** Unlike per-project gates, pair `orm.file` and `ts.file` resolve against the directory containing `graphify.toml` because a typical pair (`packages/db/...` ↔ `packages/api/...`) crosses `[[project]]` boundaries.

---

## 3. Configuration

All configuration lives under `[contract]` in `graphify.toml`.

```toml
# Global defaults (optional)
[contract]
type_map = { "jsonb" = "unknown", "numeric" = "string" }
case_rule = "snake_camel"                     # values: snake_camel | exact
unmapped_type_severity = "warning"            # warning | error

# One block per pair
[[contract.pair]]
name = "user"
orm  = { source = "drizzle", file = "packages/db/src/schema/user.ts", table = "users" }
ts   = { file   = "packages/api/src/types/user.ts", export = "UserDto" }

[[contract.pair.field_alias]]
orm = "legacy_role_code"
ts  = "roleCode"

# Singular sub-table of the most recent [[contract.pair]]
[contract.pair.ignore]
orm = ["internal_audit_id", "internal_audit_at"]
ts  = []

[[contract.pair.relation_alias]]
orm = "posts"
ts  = "posts"
```

### Shape rationale

- `orm.source` is an enum reserving future values (`prisma`, `typeorm`, ...) without breaking the schema.
- `orm.file` plus `orm.table` uniquely identifies a declared table, even when a file defines many. `ts.file` plus `ts.export` serves the same role for TS.
- Relative paths resolve against the directory containing `graphify.toml` (the workspace root). Unlike per-project gates, pairs routinely cross `[[project]]` boundaries, so workspace-root resolution is the only unambiguous rule.
- `field_alias` and `relation_alias` use TOML array-of-tables per pair (zero or more each). `ignore` is a singular sub-table per pair (one entry with `orm` and `ts` arrays). Both forms nest correctly under the most recent `[[contract.pair]]`.
- A monorepo with dozens of pairs stays readable because per-pair overrides sit directly under each `[[contract.pair]]` block.

### Validation at load time

- Both sides of every pair must resolve to declared exports; unresolved → hard error.
- Duplicate `name` values across pairs → error.
- Unknown `orm.source` → error.
- Missing `orm.table` when the file declares more than one table → error.

---

## 4. Data model

All types live in `graphify-core/src/contract.rs`, implement `Serialize`/`Deserialize`, and avoid any tree-sitter or IO dependency.

```rust
pub struct Contract {
    pub name: String,                   // pair name from config
    pub side: ContractSide,             // Orm | Ts
    pub source_file: PathBuf,
    pub source_symbol: String,          // "users" or "UserDto"
    pub fields: Vec<Field>,
    pub relations: Vec<Relation>,
}

pub enum ContractSide { Orm, Ts }

pub struct Field {
    pub name: String,                   // normalized (camelCase after snake_camel)
    pub raw_name: String,               // as written in source
    pub type_ref: FieldType,
    pub nullable: bool,
    pub has_default: bool,              // ORM-only signal, reserved for future rules
    pub line: usize,
}

pub enum FieldType {
    Primitive(PrimitiveType),
    Named(String),                      // e.g. "UserMetadata"
    Union(Vec<FieldType>),              // TS only in v1
    Array(Box<FieldType>),
    Unmapped(String),                   // raw token when type_map has no entry
}

pub enum PrimitiveType { String, Number, Boolean, Date, Unknown }

pub struct Relation {
    pub name: String,
    pub raw_name: String,
    pub cardinality: Cardinality,
    pub target_contract: String,        // advisory only; not a drift axis in v1
    pub nullable: bool,
    pub line: usize,
}

pub enum Cardinality { One, Many }

pub struct ContractComparison {
    pub pair_name: String,
    pub violations: Vec<ContractViolation>,
}

pub enum ContractViolation {
    FieldMissingOnTs     { field: String, orm_type: FieldType, line: usize },
    FieldMissingOnOrm    { field: String, ts_type: FieldType, line: usize },
    TypeMismatch         { field: String, orm: FieldType, ts: FieldType, line_orm: usize, line_ts: usize },
    NullabilityMismatch  { field: String, orm_nullable: bool, ts_nullable: bool, line_orm: usize, line_ts: usize },
    RelationMissingOnTs  { relation: String, line: usize },
    RelationMissingOnOrm { relation: String, line: usize },
    CardinalityMismatch  { relation: String, orm: Cardinality, ts: Cardinality, line_orm: usize, line_ts: usize },
    UnmappedOrmType      { field: String, raw_type: String, line: usize },
}
```

### Design notes

- A pair produces two `Contract` values. `compare_contracts(orm, ts, pair_config, global) -> ContractComparison` is a pure function.
- `FieldType::Unmapped` replaces what would have been a parser-level error with a first-class data variant. The comparison phase decides severity.
- Optional, `T | null`, and `T | undefined` on the TS side all collapse to `nullable: true`. Keeping three states produces low-signal drift ("optional vs nullable") that's rarely the actual bug.
- `has_default` is carried but not compared in v1; present so a future rule ("ORM has default, TS field must be optional") can be added without re-parsing.
- Line numbers are 1-indexed so violations can be deep-linked from editors (preparation for FEAT-015).

---

## 5. Drizzle parser

Located in `graphify-extract/src/drizzle.rs`. Operates on the TypeScript AST produced by the existing `typescript.rs` extractor.

### Recognized patterns

```ts
// Table declaration
export const users = pgTable('users', {
  id:         uuid('id').primaryKey().defaultRandom(),
  email:      text('email').notNull().unique(),
  age:        integer('age'),
  createdAt:  timestamp('created_at').defaultNow(),
  metadata:   jsonb('metadata').$type<UserMetadata>(),
});

// Relations declaration (separate call)
export const usersRelations = relations(users, ({ one, many }) => ({
  profile: one(profiles, { fields: [users.profileId], references: [profiles.id] }),
  posts:   many(posts),
}));
```

### Extraction algorithm

1. Walk the TS AST for `export const X = pgTable(...)` (also `sqliteTable`, `mysqlTable`, and `pgSchema('x').table(...)`).
2. For each table call, the second argument is an object literal. Walk its properties:
   - Property name → `Field.raw_name` (e.g., `createdAt`).
   - Property value root call → column type token (`text`, `integer`, ...).
   - First string argument of the root call → raw column name (e.g., `'created_at'`) when present. Absence means the column name equals the property name.
   - `.notNull()` in the chain → `nullable: false`; absence → `nullable: true`.
   - `.default*(...)` in the chain → `has_default: true`.
   - `.$type<Foo>()` generic → overrides to `FieldType::Named("Foo")`.
3. Walk for `export const Y = relations(Table, ({ one, many }) => ({ ... }))`:
   - First argument identifies the table symbol; the relations block attaches to that table's contract.
   - Each property in the returned object:
     - `one(Target, ...)` → `Cardinality::One`. Target identifier captured as `target_contract`.
     - `many(Target)` → `Cardinality::Many`.
   - In v1, `one(...)` relations are conservatively marked `nullable: true`. False-negatives on relation nullability are less noisy than false-positives, and the FK-column-nullability path would require cross-reference beyond v1's scope.
4. Emit one `Contract { side: Orm }` per table declaration matching a pair configuration.

### Built-in type map

| Drizzle column builder | PrimitiveType | Notes |
|---|---|---|
| `text`, `varchar`, `char`, `uuid` | String | |
| `integer`, `serial`, `bigserial`, `smallint`, `real`, `double_precision` | Number | |
| `numeric`, `decimal` | Number | overridable; runtime may be string |
| `boolean` | Boolean | |
| `timestamp`, `date`, `time` | Date | |
| `json`, `jsonb` | Unknown | unless `.$type<Foo>()` → `Named("Foo")` |
| `pgEnum('name', [...])` usage | Named("name") | enum values not compared in v1 |
| unknown call | `Unmapped(raw_token)` | carried forward, severity decided later |

### Explicitly unsupported in v1

- Spread expressions inside the column object (`{ ...base, age: integer(...) }`) — parser emits a warning and does not expand.
- Re-export barrels — `orm.file` must point at the declaring file, not a re-exporting module. Config validation catches this.
- Custom column builders (`customType({...})`) — emits `Unmapped`.
- Composite primary keys, indexes, check constraints — parsed-through but not comparison targets.

---

## 6. TypeScript parser

Located in `graphify-extract/src/ts_contract.rs`. Consumes the same TS AST.

### Recognized shapes

```ts
// interface
export interface UserDto {
  id: string;
  email: string;
  age: number | null;
  createdAt: Date;
  metadata: UserMetadata;
  profile?: ProfileDto;
  posts: PostDto[];
}

// object type alias
export type UserDto = { id: string; ... };

// intersection
export type UserDto = BaseEntity & { email: string };
```

### Extraction algorithm

1. Walk `interface_declaration` and `type_alias_declaration` nodes matching `pair.ts.export`.
2. For each property signature:
   - `name` → `raw_name`.
   - `?:` → `nullable: true`.
   - Type node → `FieldType` via type resolution table below.
3. Classify scalar vs relation in a second pass, once every configured TS contract has been parsed:
   - Type references pointing to a known TS contract → `Relation`.
   - `T[]` or `Array<T>` where `T` is a known contract → `Relation { cardinality: Many }`.
   - Single known-contract references → `Relation { cardinality: One }`.
   - Everything else → `Field`.

### Type resolution

| TS type node | FieldType |
|---|---|
| `string` | `Primitive(String)` |
| `number`, `bigint` | `Primitive(Number)` |
| `boolean` | `Primitive(Boolean)` |
| `Date` | `Primitive(Date)` |
| `null`, `undefined` | collapsed into `nullable: true` |
| `T \| null`, `T \| undefined` | strip null-side, flag nullable |
| `T[]`, `Array<T>` | `Array(resolve(T))` |
| `T \| U` (non-null union) | `Union([resolve(T), resolve(U)])` |
| Named type (non-contract) | `Named(name)` |
| `Record<string, unknown>`, `unknown`, `any` | `Primitive(Unknown)` |

### Intersection handling

- `A & B` where both are inline object literals: merge property sets, later keys win on name collisions.
- `A & B` where one side is a reference to another declared type: resolved only if that type is parsed locally. Otherwise the property set is emitted as-is and a warning records the unresolved partner.
- Chains of three or more intersections are flattened left-to-right.

### Explicitly unsupported in v1

- Mapped types (`{ [K in keyof X]: ... }`) — emits `Unmapped`, warns.
- Conditional types — same.
- Utility types other than `Array<T>` (`Partial`, `Omit`, `Pick`, `Readonly`) — parsed as `Named`; future work can expand.
- Generic interfaces (`interface X<T>`) — skipped with a warning at config load.
- Declaration merging across multiple `interface X` blocks — first declaration wins, duplicates warn.

---

## 7. Comparison algorithm

Implemented as a pure function in `graphify-core/src/contract.rs`:

```rust
pub fn compare_contracts(
    orm: &Contract,
    ts:  &Contract,
    pair_config: &PairConfig,
    global: &GlobalContractConfig,
) -> ContractComparison;
```

### Phase 1 — Field alignment

1. Apply `pair_config.ignore.orm` and `pair_config.ignore.ts` to both field sets.
2. Build two maps keyed by normalized name:
   - ORM: apply aliases first, then `case_rule` (`snake_camel` converts `created_at` → `createdAt`).
   - TS: apply aliases. `case_rule = exact` disables normalization.
3. `orm_only` → `FieldMissingOnTs`; `ts_only` → `FieldMissingOnOrm`.
4. Continue with the intersection.

### Phase 2 — Per-field comparison

For each shared normalized name, emit in order:

1. **Nullability check.** Mismatch → `NullabilityMismatch`. Continue to the type check; violations accumulate per field.
2. **Type check.** Map ORM `FieldType` through the effective type map (project override wins, then built-in default, then `Unmapped`).
   - `Unmapped` on the ORM side → `UnmappedOrmType` (warning by default). Skip the type comparison for that field.
   - Comparison:
     - `Primitive(A) == Primitive(B)` iff `A == B`.
     - `Named(A) == Named(B)` iff `A == B` (trust `.$type<Foo>()` ↔ TS `Foo`).
     - `Array(A) == Array(B)` iff `A == B` (recursive).
     - `Primitive(Unknown)` on either side matches anything — explicit escape hatch.
     - `Union` only appears on the TS side in v1; matching a TS union against an ORM `Primitive` is always a mismatch.
   - Mismatch → `TypeMismatch`.

### Phase 3 — Relation alignment

Same alignment structure as Phase 1, keyed on relation names (same case rule).

- ORM-only → `RelationMissingOnTs`.
- TS-only → `RelationMissingOnOrm`.
- Shared → compare cardinality. Mismatch → `CardinalityMismatch`.
- Relation `nullable` is NOT compared in v1 (see Drizzle parser section).
- `target_contract` is NOT compared in v1; DTOs legitimately reshape relations (e.g., `profile: ProfileSummary`), and surfacing that as drift destroys signal-to-noise.

### Phase 4 — Severity and ordering

| Violation | Default severity |
|---|---|
| All `*Missing*`, `*Mismatch*` | error |
| `UnmappedOrmType` | warning (configurable via `unmapped_type_severity` or `--contracts-warnings-as-errors`) |

Violations sort by `(source_file, line, variant_rank)` for deterministic output. Any error causes `graphify check` to exit non-zero; warnings alone do not.

### Complexity

- Per pair: `O(|fields| + |relations|)` using hashmap lookups.
- Trivially parallelizable per pair via `rayon` if needed; v1 stays sequential.

---

## 8. Output

Contract findings merge into the existing `graphify check` output from FEAT-004/FEAT-013. No standalone report file.

### Human output

```
graphify check — 2 failing gate(s)

Cycles: OK
Hotspots: OK
Policy rules: OK
Contract drift: FAILED (3 errors, 1 warning across 2 pair(s))

  pair: user  (packages/db/src/schema/user.ts:12 ↔ packages/api/src/types/user.ts:5)
    error   FieldMissingOnTs         phone       orm line 18: text('phone').notNull()
    error   NullabilityMismatch      age         orm nullable=false, ts nullable=true
                                                 orm line 22, ts line 9
    warning UnmappedOrmType          tags        raw orm type: 'tsvector'
                                                 orm line 31

  pair: post  (packages/db/src/schema/post.ts:8 ↔ packages/api/src/types/post.ts:4)
    error   CardinalityMismatch      author      orm=One, ts=Many
                                                 orm line 14, ts line 11

Exit code: 1
```

Colors follow the FEAT-004/FEAT-013 palette: `error` red, `warning` yellow, pair headers dim. `termcolor` crate already a dependency.

### JSON output (`report/check.json` and per-project reports)

The existing `CheckReport` shape is:

```rust
struct CheckReport {
    ok: bool,
    violations: usize,
    projects: Vec<ProjectCheckResult>,
}
```

Contract drift is additive: a new optional `contracts: Option<ContractCheckResult>` field alongside `projects`. Existing consumers that only read `projects[].violations[]` see no change.

```json
{
  "ok": false,
  "violations": 3,
  "projects": [
    { "name": "db",  "ok": true, "summary": { "...": "..." }, "violations": [] },
    { "name": "api", "ok": true, "summary": { "...": "..." }, "violations": [] }
  ],
  "contracts": {
    "ok": false,
    "error_count": 3,
    "warning_count": 1,
    "pairs": [
      {
        "name": "user",
        "orm": { "file": "packages/db/src/schema/user.ts", "symbol": "users",   "line": 12 },
        "ts":  { "file": "packages/api/src/types/user.ts",  "symbol": "UserDto", "line": 5  },
        "violations": [
          { "kind": "contract_field_missing_on_ts",  "severity": "error",   "field": "phone", "orm_type": { "kind": "primitive", "value": "String" }, "orm_line": 18 },
          { "kind": "contract_nullability_mismatch", "severity": "error",   "field": "age",   "orm_nullable": false, "ts_nullable": true, "orm_line": 22, "ts_line": 9 },
          { "kind": "contract_unmapped_orm_type",    "severity": "warning", "field": "tags",  "raw_type": "tsvector", "orm_line": 31 }
        ]
      }
    ]
  }
}
```

Schema conventions:

- `violations[].kind` is snake_case with a `contract_` prefix — matches FEAT-013's `kind: "policy_rule"` convention and namespaces contract violations so consumers can filter by prefix.
- File paths are workspace-root relative (see Architecture).
- Line numbers are 1-indexed.
- Pair-level `line` points at the declaration of the table/type (useful for IDE jump).
- Top-level `violations` count includes contract errors so downstream tooling reading that single number still gets a correct aggregate failure signal. Contract warnings are NOT counted in that total.

### CLI flags on `graphify check`

| Flag | Effect |
|---|---|
| `--contracts` / `--no-contracts` | Explicit opt-in/out. Default: run iff `[[contract.pair]]` declared. |
| `--contracts-warnings-as-errors` | Escalate `UnmappedOrmType` (and any future warning-class violations) to errors. |
| `--json` | Existing flag. Contracts appear inside `check.json`. |

### Markdown report (`architecture_report.md`)

A new `## Contract Drift` section appears only when pairs are configured. Per pair: declaration lines, then a table of violations with `severity | kind | field | details` columns.

### HTML report (FEAT-001)

New collapsed panel "Contracts" alongside existing panels, rendering the same table. Low-cost extension of the existing panel pattern.

### Not extended in v1

- Neo4j, GraphML, Obsidian export formats (they describe graph structure, not drift axes).
- Trend reports (FEAT-014) — contract drift is a spot check, not a time-series metric in v1.

---

## 9. Testing strategy

Three tiers matching the existing workspace layout. Target: ~60–80 new tests across the workspace.

### Tier 1 — Pure comparison (`graphify-core`)

`#[cfg(test)]` in `contract.rs`. No tree-sitter, no IO. Hand-constructed `Contract` values via a `ContractBuilder`.

Coverage matrix (every violation × positive and negative case):

- Identical contracts → 0 violations.
- ORM-only field → `FieldMissingOnTs`.
- TS-only field → `FieldMissingOnOrm`.
- `text` vs `number` → `TypeMismatch`.
- Nullability mismatch in both directions.
- Optional TS (`?:`) collapse matches ORM nullable.
- `jsonb` with `$type<Foo>()` vs TS `Foo` → 0 violations.
- `tsvector` unmapped → `UnmappedOrmType`, not `TypeMismatch`.
- `unmapped_type_severity = error` escalates the warning.
- Field aliases on both sides resolve to 0 violations.
- Ignored fields not reported.
- Relation `one` vs `many` → `CardinalityMismatch`.
- Relation `one` vs `one` with different `target_contract` → 0 violations (advisory only).
- Missing relations in either direction.
- `snake_camel` case rule maps `created_at` ↔ `createdAt`.
- Deterministic ordering by `(file, line, variant_rank)`.

### Tier 2 — Parser tests (`graphify-extract`)

`#[cfg(test)]` in `drizzle.rs` and `ts_contract.rs`. Inline TS source strings as fixtures.

Drizzle:

- Scalar table with every type in the default map.
- `.notNull()` on/off.
- `.$type<Foo>()` override.
- `relations()` block with `one()` and `many()`.
- Unknown column type → `Unmapped`.
- `pgSchema('auth').table(...)`.
- Multiple tables per file; parser selects by the configured `table`.
- Non-table exports in the same file are ignored without error.
- Spread (`{ ...base, ... }`) warns and does not expand.

TS:

- `interface` and `type` produce identical output.
- Optional (`?:`), `T | null`, `T | undefined` all collapse to nullable.
- `T[]` and `Array<T>` produce the same `Array` variant.
- Intersection of two inline objects flattens.
- Intersection with an unparsed external type warns and keeps the inline side.
- Nested contract reference → `Relation`.
- Nested `PostDto[]` → `Relation { cardinality: Many }`.
- Declaration merging: first declaration wins, duplicates warn.

### Tier 3 — End-to-end integration (`tests/contract_integration.rs`)

Uses the `OnceLock`-guarded `graphify_bin()` harness introduced this session. Fixture: `tests/fixtures/contract_drift/monorepo/` — a 2-project layout with `packages/db` and `packages/api`, plus a `graphify.toml` declaring two pairs (one clean, one drifted).

Cases:

1. `graphify check` exits 0 on the clean pair alone.
2. `graphify check` exits 1 on the drifted pair; human output contains the expected violation kinds.
3. `graphify check --json` produces a schema-conformant `check.json` with the top-level `contracts` block populated and existing `projects[].violations[]` untouched.
4. `graphify check --no-contracts` skips the gate even when pairs are configured.
5. `graphify check --contracts-warnings-as-errors` escalates `UnmappedOrmType` to a failing exit code.
6. Config validation: missing `orm.file` produces a clear error at load.
7. Config validation: `ts.export` not found in file produces a clear error at load.
8. Idempotent output: same input → byte-for-byte identical `check.json`.
9. Incremental cache: second run with unchanged fixtures reads from cache, still emits correct violations (regression guard against FEAT-005 interference).

### Snapshot choices

- JSON: `serde_json::to_string_pretty` compared against a committed fixture. Deterministic ordering makes this stable.
- Human and Markdown output: substring assertions only. ANSI codes and whitespace are too brittle for snapshot equality.

### Performance guardrail

One test generates a 100-pair fixture programmatically and asserts `graphify check` completes under 2 seconds on a cold cache and under 500 ms warm. Catches accidental `O(n²)` regressions in alignment.

### Not tested in v1

- Real-world third-party Drizzle schemas (fixtures stay synthetic).
- Prisma and TypeORM parsers (not shipped).
- Multi-project pair references (pair declared in project A with TS type in project B) — deferred to v2 with workspace-wide resolution.

---

## 10. Open questions deferred to v2

- Prisma and TypeORM parsers behind the same `orm.source` enum.
- Zod and tRPC as additional TS-side sources.
- Cross-project pair references with workspace-aware resolution.
- Relation nullability comparison (requires FK-column cross-reference on the ORM side).
- `target_contract` reconciliation (requires distinguishing "same entity, reshaped" from "wrong entity").
- Defaults, indexes, and constraint comparisons as new drift classes.
- Mapped and conditional types in TS.
- Contract history over time (integration with FEAT-014 trends).

---

## 11. References

- FEAT-002 — Architectural drift detection (spec for analogous pure-comparison design).
- FEAT-004 — CI quality gates (existing `graphify check` surface).
- FEAT-005 — Incremental builds with SHA256 cache (ExtractionResult caching path).
- FEAT-008 — Confidence scoring (pattern of carrying uncertainty through the pipeline as data).
- FEAT-013 — Policy-driven architecture rules (analogous extension of `check` with JSON gate output).
- FEAT-014 — Historical architecture trend tracking (pattern of implicit opt-in via config presence).
