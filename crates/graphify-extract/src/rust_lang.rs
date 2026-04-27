use crate::lang::{ExtractionResult, LanguageExtractor, ReExportEntry, ReExportSpec};
use graphify_core::types::{Edge, Language, Node, NodeKind};
use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// RustExtractor
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RustExtractor;

impl RustExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageExtractor for RustExtractor {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to load Rust grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Every file gets a module node.
        result
            .nodes
            .push(Node::module(module_name, path, Language::Rust, 1, true));

        // Walk top-level statements.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "use_declaration" => {
                    extract_use_declaration(&child, source, module_name, &mut result);
                }
                "mod_item" => {
                    extract_mod_item(&child, source, module_name, &mut result);
                }
                "function_item" => {
                    extract_function_item(&child, source, path, module_name, &mut result);
                }
                "struct_item" => {
                    extract_struct_item(&child, source, path, module_name, &mut result);
                }
                "enum_item" => {
                    extract_enum_item(&child, source, path, module_name, &mut result);
                }
                "trait_item" => {
                    extract_trait_item(&child, source, path, module_name, &mut result);
                }
                "type_item" => {
                    extract_type_item(&child, source, path, module_name, &mut result);
                }
                "static_item" | "const_item" => {
                    extract_value_item(&child, source, path, module_name, &mut result);
                }
                "impl_item" => {
                    extract_impl_item(&child, source, path, module_name, &mut result);
                }
                "macro_invocation" => {
                    extract_macro_invocation(&child, source, module_name, &mut result);
                }
                _ => {
                    // Top-level fallback — no enclosing function, so no
                    // local-binding scope to honor (BUG-024).
                    let empty: HashSet<String> = HashSet::new();
                    extract_calls_recursive(&child, source, module_name, &empty, &mut result);
                }
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Use declaration extraction
// ---------------------------------------------------------------------------

fn extract_use_declaration(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // The `argument` field contains the use path (e.g., scoped_identifier,
    // scoped_use_list, identifier, etc.).
    if let Some(arg) = node.child_by_field_name("argument") {
        let line = node.start_position().row + 1;
        collect_use_paths(&arg, source, module_name, line, result);

        // FEAT-045: when the use_declaration is `pub use …` (or a restricted
        // pub variant like `pub(crate) use …`), additionally emit one or more
        // `ReExportEntry` records so the project-wide re-export graph can
        // walk the canonical declaration. The existing `collect_use_paths`
        // call above is left in place: it still produces the Imports edge
        // and `use_aliases` entry that the rest of the pipeline depends on,
        // exactly as for non-pub `use`. Re-exports are an additive signal,
        // not a replacement.
        if has_pub_visibility(node, source) {
            collect_pub_use_reexports(&arg, source, module_name, line, result);
        }
    }
}

/// Returns `true` when the `use_declaration` node carries a visibility
/// modifier whose leading token is `pub` — i.e. the use re-exports its
/// items. Covers `pub use`, `pub(crate) use`, `pub(super) use`, etc.
///
/// The `visibility_modifier` is the FIRST named child of `use_declaration`
/// when present (sibling to the `use` keyword token, not wrapping the
/// declaration itself). Verified against tree-sitter-rust grammar.
fn has_pub_visibility(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(source).unwrap_or("");
            return text.starts_with("pub");
        }
    }
    false
}

/// Recursively collect import paths from use declaration arguments.
/// Handles: identifiers, scoped_identifier, scoped_use_list, use_as_clause, use_list.
///
/// FEAT-031: also populates `result.use_aliases` with `short_name → full_path`
/// entries so the post-extraction resolver can rewrite scoped and bare-name
/// call targets (e.g. `Node::module` after `use crate::types::Node;`).
fn collect_use_paths(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "identifier" | "scoped_identifier" | "crate" | "self" | "super" => {
            let path_str = node.utf8_text(source).unwrap_or("").to_owned();
            if !path_str.is_empty() {
                register_use_alias(&path_str, None, result);
                result
                    .edges
                    .push((module_name.to_owned(), path_str, Edge::imports(line)));
            }
        }
        "use_as_clause" => {
            // `use foo::bar as baz` — the path is the first child.
            if let Some(path_node) = node.child_by_field_name("path") {
                let path_str = path_node.utf8_text(source).unwrap_or("").to_owned();
                if !path_str.is_empty() {
                    let alias = node
                        .child_by_field_name("alias")
                        .and_then(|n| n.utf8_text(source).ok())
                        .filter(|s| !s.is_empty());
                    register_use_alias(&path_str, alias, result);
                    result
                        .edges
                        .push((module_name.to_owned(), path_str, Edge::imports(line)));
                }
            }
        }
        "scoped_use_list" => {
            // `use std::{io, fs}` — path prefix + list of imports.
            let prefix = node
                .child_by_field_name("path")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if let Some(list_node) = node.child_by_field_name("list") {
                process_scoped_use_list(&list_node, source, module_name, line, prefix, result);
            }
        }
        "use_list" => {
            // Bare use list (rare, e.g. `use {foo, bar}`).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_use_paths(&child, source, module_name, line, result);
                }
            }
        }
        "use_wildcard" => {
            // `use foo::*` — import the path without the wildcard. Wildcard
            // short-name expansion is v2 (FEAT-031 boundaries) since the set
            // of names is not visible at parse time.
            let text = node.utf8_text(source).unwrap_or("");
            let path = text.strip_suffix("::*").unwrap_or(text);
            if !path.is_empty() {
                result
                    .edges
                    .push((module_name.to_owned(), path.to_owned(), Edge::imports(line)));
            }
        }
        _ => {}
    }
}

/// Process the items inside a `scoped_use_list`'s `list` field with a given
/// prefix. Recurses into nested `scoped_use_list` children with the combined
/// prefix (BUG-023: previously captured nested groups as literal text including
/// braces, so `use a::{b::{c, d}}` produced a single `a::b::{c, d}` edge with
/// no aliases registered for `c`/`d`).
fn process_scoped_use_list(
    list_node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    prefix: &str,
    result: &mut ExtractionResult,
) {
    let join = |suffix: &str| -> String {
        if prefix.is_empty() {
            suffix.to_owned()
        } else {
            format!("{}::{}", prefix, suffix)
        }
    };

    let mut cursor = list_node.walk();
    for child in list_node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "identifier" | "self" => {
                let name = child.utf8_text(source).unwrap_or("");
                if !name.is_empty() {
                    let full_path = join(name);
                    register_use_alias(&full_path, None, result);
                    result
                        .edges
                        .push((module_name.to_owned(), full_path, Edge::imports(line)));
                }
            }
            "scoped_identifier" => {
                let child_text = child.utf8_text(source).unwrap_or("");
                if !child_text.is_empty() {
                    let full_path = join(child_text);
                    register_use_alias(&full_path, None, result);
                    result
                        .edges
                        .push((module_name.to_owned(), full_path, Edge::imports(line)));
                }
            }
            "use_as_clause" => {
                // Nested aliased item inside a grouped import,
                // e.g. `use std::{io::Result as IoResult};`.
                if let Some(path_node) = child.child_by_field_name("path") {
                    let path_str = path_node.utf8_text(source).unwrap_or("");
                    if !path_str.is_empty() {
                        let full_path = join(path_str);
                        let alias = child
                            .child_by_field_name("alias")
                            .and_then(|n| n.utf8_text(source).ok())
                            .filter(|s| !s.is_empty());
                        register_use_alias(&full_path, alias, result);
                        result
                            .edges
                            .push((module_name.to_owned(), full_path, Edge::imports(line)));
                    }
                }
            }
            "scoped_use_list" => {
                let inner_path = child
                    .child_by_field_name("path")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                let combined_prefix = join(inner_path);
                if let Some(inner_list) = child.child_by_field_name("list") {
                    process_scoped_use_list(
                        &inner_list,
                        source,
                        module_name,
                        line,
                        &combined_prefix,
                        result,
                    );
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// FEAT-045: pub use → ReExportEntry emission
// ---------------------------------------------------------------------------

/// Walk the `argument` of a `pub use …` declaration and append one
/// `ReExportEntry` per distinct `raw_target` (the path prefix shared by a
/// group of re-exported leaves). Invoked only when [`has_pub_visibility`]
/// returned `true`.
///
/// Mapping rules (per FEAT-045 AC):
/// - `pub use foo::bar::Baz;`        → `raw_target = "foo::bar"`, one spec `{Baz, Baz}`
/// - `pub use foo::bar::Baz as Qux;` → `raw_target = "foo::bar"`, one spec `{Baz, Qux}`
/// - `pub use foo::{Bar, Baz};`      → `raw_target = "foo"`,      two specs
/// - Nested grouped imports recurse via [`collect_pub_use_specs_in_list`]
///   with the combined prefix; each combined prefix produces its own
///   `ReExportEntry`.
/// - `pub use foo;` (single segment) — nothing to re-export from a non-empty
///   prefix; emitted as nothing. Same boundary the existing import path
///   tolerates.
/// - `pub use foo::*;` (wildcards) — explicitly out of scope (FEAT-045 v1).
fn collect_pub_use_reexports(
    arg: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    result: &mut ExtractionResult,
) {
    match arg.kind() {
        "scoped_identifier" => {
            let full = arg.utf8_text(source).unwrap_or("");
            if let Some((prefix, leaf)) = split_scoped_path(full) {
                push_reexport_entry(
                    result,
                    module_name,
                    prefix,
                    line,
                    vec![ReExportSpec {
                        exported_name: leaf.to_owned(),
                        local_name: leaf.to_owned(),
                    }],
                );
            }
        }
        "use_as_clause" => {
            // `pub use foo::bar::Baz as Qux;`
            let path_text = arg
                .child_by_field_name("path")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            let alias = arg
                .child_by_field_name("alias")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if let Some((prefix, leaf)) = split_scoped_path(path_text) {
                let local = if alias.is_empty() { leaf } else { alias };
                push_reexport_entry(
                    result,
                    module_name,
                    prefix,
                    line,
                    vec![ReExportSpec {
                        exported_name: leaf.to_owned(),
                        local_name: local.to_owned(),
                    }],
                );
            }
        }
        "scoped_use_list" => {
            // `pub use foo::{Bar, Baz};` and nested groups.
            let prefix = arg
                .child_by_field_name("path")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if let Some(list_node) = arg.child_by_field_name("list") {
                // Accumulate `(combined_prefix, ReExportSpec)` for every
                // leaf in this group (recursing through nested groups), then
                // bucket by combined_prefix and push one ReExportEntry per
                // bucket. Order-stable: leaves keep grammar order, nested
                // groups expand inline.
                let mut buckets: Vec<(String, Vec<ReExportSpec>)> = Vec::new();
                collect_pub_use_specs_in_list(&list_node, source, prefix, &mut buckets);
                for (raw_target, specs) in buckets {
                    push_reexport_entry(result, module_name, &raw_target, line, specs);
                }
            }
        }
        // `identifier`, `crate`, `self`, `super`, `use_list`, `use_wildcard` —
        // not supported by FEAT-045 v1. `use_wildcard` is explicit OOS;
        // bare-identifier `pub use foo;` has no source-side prefix and so
        // does not fit the canonical-collapse model.
        _ => {}
    }
}

/// Recursive walker for the `list` field of a `scoped_use_list` (or a
/// nested `scoped_use_list` inside it). Appends specs into `buckets`,
/// keyed by the combined prefix (so a single `pub use foo::{bar::Baz, Qux};`
/// produces two buckets: `"foo::bar"` and `"foo"`).
fn collect_pub_use_specs_in_list(
    list_node: &tree_sitter::Node,
    source: &[u8],
    prefix: &str,
    buckets: &mut Vec<(String, Vec<ReExportSpec>)>,
) {
    let mut cursor = list_node.walk();
    for child in list_node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "identifier" | "self" => {
                let name = child.utf8_text(source).unwrap_or("");
                if !name.is_empty() && !prefix.is_empty() {
                    push_spec_into_bucket(
                        buckets,
                        prefix,
                        ReExportSpec {
                            exported_name: name.to_owned(),
                            local_name: name.to_owned(),
                        },
                    );
                }
            }
            "scoped_identifier" => {
                // `pub use foo::{bar::Baz};` — the leaf is `Baz`, the prefix
                // becomes `foo::bar`.
                let text = child.utf8_text(source).unwrap_or("");
                if let Some((inner_prefix, leaf)) = split_scoped_path(text) {
                    let combined = combine_prefix(prefix, inner_prefix);
                    push_spec_into_bucket(
                        buckets,
                        &combined,
                        ReExportSpec {
                            exported_name: leaf.to_owned(),
                            local_name: leaf.to_owned(),
                        },
                    );
                }
            }
            "use_as_clause" => {
                // `pub use foo::{Bar as Baz};` — the path's last segment is
                // the exported name; the alias is the local name.
                let path_text = child
                    .child_by_field_name("path")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                let alias = child
                    .child_by_field_name("alias")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                if path_text.contains("::") {
                    // e.g. `bar::Baz as Qux` — split into prefix + leaf.
                    if let Some((inner_prefix, leaf)) = split_scoped_path(path_text) {
                        let combined = combine_prefix(prefix, inner_prefix);
                        let local = if alias.is_empty() { leaf } else { alias };
                        push_spec_into_bucket(
                            buckets,
                            &combined,
                            ReExportSpec {
                                exported_name: leaf.to_owned(),
                                local_name: local.to_owned(),
                            },
                        );
                    }
                } else if !path_text.is_empty() && !prefix.is_empty() {
                    // e.g. `Bar as Baz` — leaf is `Bar`, prefix unchanged.
                    let local = if alias.is_empty() { path_text } else { alias };
                    push_spec_into_bucket(
                        buckets,
                        prefix,
                        ReExportSpec {
                            exported_name: path_text.to_owned(),
                            local_name: local.to_owned(),
                        },
                    );
                }
            }
            "scoped_use_list" => {
                // `pub use foo::{bar::{Baz, Qux}};`
                let inner_path = child
                    .child_by_field_name("path")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                let combined_prefix = combine_prefix(prefix, inner_path);
                if let Some(inner_list) = child.child_by_field_name("list") {
                    collect_pub_use_specs_in_list(&inner_list, source, &combined_prefix, buckets);
                }
            }
            _ => {}
        }
    }
}

/// Append `spec` into the bucket for `prefix` (creating one if needed).
/// First-occurrence wins for bucket ordering (matches grammar order).
fn push_spec_into_bucket(
    buckets: &mut Vec<(String, Vec<ReExportSpec>)>,
    prefix: &str,
    spec: ReExportSpec,
) {
    for (key, specs) in buckets.iter_mut() {
        if key == prefix {
            specs.push(spec);
            return;
        }
    }
    buckets.push((prefix.to_owned(), vec![spec]));
}

/// Split a `::`-scoped path into `(prefix, leaf)`. Returns `None` for
/// single-segment paths (no prefix to re-export from).
fn split_scoped_path(path: &str) -> Option<(&str, &str)> {
    let idx = path.rfind("::")?;
    let (prefix, rest) = path.split_at(idx);
    let leaf = &rest[2..]; // skip the "::"
    if prefix.is_empty() || leaf.is_empty() {
        None
    } else {
        Some((prefix, leaf))
    }
}

/// Combine a parent prefix with an inner-group prefix using `::` joiner.
/// Empty inputs are tolerated (rare; defensive).
fn combine_prefix(outer: &str, inner: &str) -> String {
    match (outer.is_empty(), inner.is_empty()) {
        (true, _) => inner.to_owned(),
        (_, true) => outer.to_owned(),
        _ => format!("{}::{}", outer, inner),
    }
}

/// Push a single ReExportEntry into `result.reexports` if it has at least
/// one spec and a non-empty raw_target. Pure helper.
fn push_reexport_entry(
    result: &mut ExtractionResult,
    module_name: &str,
    raw_target: &str,
    line: usize,
    specs: Vec<ReExportSpec>,
) {
    if raw_target.is_empty() || specs.is_empty() {
        return;
    }
    result.reexports.push(ReExportEntry {
        from_module: module_name.to_owned(),
        raw_target: raw_target.to_owned(),
        line,
        specs,
        is_star: false,
    });
}

/// Register a `short_name → full_path` entry in the extraction's `use_aliases`
/// map (FEAT-031).
///
/// The short name comes from `alias_override` when present (i.e. the local
/// name introduced by a `use … as …` clause); otherwise it's the last
/// `::`-separated segment of `full_path`. Single-segment paths (`use foo;`)
/// also register `foo → foo` — harmless, and keeps the fallback logic
/// uniform in the resolver.
fn register_use_alias(
    full_path: &str,
    alias_override: Option<&str>,
    result: &mut ExtractionResult,
) {
    let short = match alias_override {
        Some(alias) => alias,
        None => full_path.rsplit("::").next().unwrap_or(full_path),
    };
    if short.is_empty() {
        return;
    }
    // First `use` wins — subsequent duplicates are unusual (would be a compile
    // error in Rust) and we don't want to silently overwrite the original.
    result
        .use_aliases
        .entry(short.to_owned())
        .or_insert_with(|| full_path.to_owned());
}

// ---------------------------------------------------------------------------
// Module declaration
// ---------------------------------------------------------------------------

fn extract_mod_item(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // `mod name;` (file-level) or `mod name { ... }` (inline).
    // Only emit an Imports edge for file-level mod declarations (no body).
    let has_body = node.child_by_field_name("body").is_some();
    if has_body {
        return; // Inline mod — items are already in this file's AST.
    }

    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let mod_name = name_node.utf8_text(source).unwrap_or("");
    if mod_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    result.edges.push((
        module_name.to_owned(),
        mod_name.to_owned(),
        Edge::imports(line),
    ));
}

// ---------------------------------------------------------------------------
// Function item
// ---------------------------------------------------------------------------

fn extract_function_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let func_name = name_node.utf8_text(source).unwrap_or("");
    if func_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    let symbol_id = format!("{}.{}", module_name, func_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Function,
        path,
        Language::Rust,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan function body for call sites. Pre-scan local bindings so closure
    // and let-bound names don't emit bogus Calls edges (BUG-024).
    if let Some(body) = node.child_by_field_name("body") {
        let local_bindings = collect_local_bindings(&body, source);
        // BUG-025: function-body `use` declarations must register their
        // aliases too, otherwise bare callees like `Bar::new()` after a
        // `use foo::Bar;` inside the function look external.
        walk_for_uses(&body, source, module_name, result);
        extract_calls_recursive(&body, source, module_name, &local_bindings, result);
    }
}

// ---------------------------------------------------------------------------
// Struct, Enum, Trait items
// ---------------------------------------------------------------------------

fn extract_struct_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Class, result);
}

fn extract_enum_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Enum, result);

    // BUG-027: also emit Defines for each variant so bare `Selector::Group(...)`
    // callsites that resolver case 8.6 (BUG-022) synthesizes as
    // `{module}.{Enum}.{Variant}` find a hit in `known_modules`.
    let enum_name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if enum_name.is_empty() {
        return;
    }
    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };
    let mut cursor = body.walk();
    for variant in body.children(&mut cursor) {
        if variant.kind() != "enum_variant" {
            continue;
        }
        let variant_name = variant
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if variant_name.is_empty() {
            continue;
        }
        let line = variant.start_position().row + 1;
        let variant_id = format!("{}.{}.{}", module_name, enum_name, variant_name);
        result.nodes.push(Node::symbol(
            &variant_id,
            NodeKind::Class,
            path,
            Language::Rust,
            line,
            true,
        ));
        result
            .edges
            .push((module_name.to_owned(), variant_id, Edge::defines(line)));
    }
}

fn extract_trait_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Trait, result);
}

// FEAT-049: `pub type X = Y;` (and the rare non-pub `type X = Y;`) had no
// extractor handler, so the alias never landed in `known_modules` via BUG-018's
// Defines-target seeding. Consumer references like `use crate::Cycle;` then
// resolved to a non-local placeholder (`src.Cycle`) at confidence ≤ 0.5, and
// `graphify suggest stubs` falsely promoted the alias as an external prefix.
// Reuses `extract_named_type` exactly as struct/enum/trait do — `type_item`'s
// `name` field carries the alias short-name (`type_identifier`) regardless of
// the RHS shape (`generic_type`, `scoped_type_identifier`, `tuple_type`,
// `reference_type`, …), so no RHS parsing is needed for the local-symbol
// registration path. RHS canonical-collapse (mapping `Foo` → `mod::Bar` when
// the RHS is a path) is intentionally out of scope: the actual dogfood case
// (`pub type Cycle = Vec<String>;`) has a generic RHS that doesn't fit the
// barrel-collapse model. NodeKind::Class is reused because the enum has no
// `TypeAlias` variant; adding one would cascade through every report writer
// and match arm in the workspace.
fn extract_type_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Class, result);
}

// BUG-027: `static FOO: T = …;` and `const FOO: T = …;` had no extractor handler,
// so the canonical id never landed in `known_modules` via BUG-018's seeding pass.
// Consumer references (e.g. `use crate::install::copy_plan::INTEGRATIONS;`) then
// resolved to a non-local placeholder, surfacing in `graphify suggest stubs`.
// Both grammar nodes carry a `name` field of `identifier` type, so the same
// `extract_named_type` helper handles them — the body / RHS shape is irrelevant
// for local-symbol registration. NodeKind::Class is reused for parity with
// FEAT-049's `type_item` handler (same trade-off documented there).
fn extract_value_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Class, result);
}

fn extract_named_type(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    kind: NodeKind,
    result: &mut ExtractionResult,
) {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let type_name = name_node.utf8_text(source).unwrap_or("");
    if type_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    let symbol_id = format!("{}.{}", module_name, type_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind,
        path,
        Language::Rust,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));
}

// ---------------------------------------------------------------------------
// Impl block
// ---------------------------------------------------------------------------

fn extract_impl_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // Extract the type being implemented.
    let type_name = node
        .child_by_field_name("type")
        .and_then(|n| {
            // The type field can be type_identifier, generic_type, etc.
            // For generic_type, get the base type.
            if n.kind() == "generic_type" || n.kind() == "scoped_type_identifier" {
                // Walk to find the type_identifier child.
                let mut cursor = n.walk();
                for child in n.children(&mut cursor) {
                    if child.kind() == "type_identifier" {
                        return child.utf8_text(source).ok();
                    }
                }
                n.utf8_text(source).ok()
            } else {
                n.utf8_text(source).ok()
            }
        })
        .unwrap_or("");

    // Walk the body (declaration_list) for function_items.
    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item" {
            let method_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if method_name.is_empty() {
                continue;
            }

            let line = child.start_position().row + 1;

            let symbol_id = if type_name.is_empty() {
                format!("{}.{}", module_name, method_name)
            } else {
                format!("{}.{}.{}", module_name, type_name, method_name)
            };

            result.nodes.push(Node::symbol(
                &symbol_id,
                NodeKind::Method,
                path,
                Language::Rust,
                line,
                true,
            ));
            result
                .edges
                .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

            // Scan method body for call sites. Pre-scan local bindings —
            // per-method scope, fresh set per method (BUG-024).
            if let Some(fn_body) = child.child_by_field_name("body") {
                let local_bindings = collect_local_bindings(&fn_body, source);
                // BUG-025: same `use_declaration` walking applies inside
                // impl method bodies.
                walk_for_uses(&fn_body, source, module_name, result);
                extract_calls_recursive(&fn_body, source, module_name, &local_bindings, result);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Macro invocation
// ---------------------------------------------------------------------------

fn extract_macro_invocation(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let macro_node = match node.child_by_field_name("macro") {
        Some(n) => n,
        None => return,
    };

    // Only extract simple identifier macros (e.g. `println!`, `vec!`).
    // Skip scoped macros (e.g. `std::println!`).
    if macro_node.kind() != "identifier" {
        return;
    }

    let macro_name = macro_node.utf8_text(source).unwrap_or("");
    if macro_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    result.edges.push((
        module_name.to_owned(),
        macro_name.to_owned(),
        Edge::calls(line).with_confidence(0.6, graphify_core::types::ConfidenceKind::Inferred),
    ));
}

// ---------------------------------------------------------------------------
// Call extraction
// ---------------------------------------------------------------------------

fn extract_calls_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    local_bindings: &HashSet<String>,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                match func.kind() {
                    // Bare call: `foo()` — target is the identifier.
                    "identifier" => {
                        let callee = func.utf8_text(source).unwrap_or("").to_owned();
                        // BUG-024: skip if `callee` is a closure/let binding
                        // local to the enclosing function. The pre-scan
                        // populates `local_bindings` per-function/per-method,
                        // so a binding in fn a() does not affect fn b().
                        if !callee.is_empty() && !local_bindings.contains(&callee) {
                            let line = node.start_position().row + 1;
                            result.edges.push((
                                module_name.to_owned(),
                                callee,
                                Edge::calls(line).with_confidence(
                                    0.7,
                                    graphify_core::types::ConfidenceKind::Inferred,
                                ),
                            ));
                        }
                    }
                    // Scoped call: `Foo::bar()`, `foo::Bar::baz()` (FEAT-031).
                    // Target is the full scoped path verbatim; the post-
                    // extraction resolver's `use_aliases` fallback rewrites
                    // the root segment to the canonical local symbol.
                    "scoped_identifier" => {
                        let callee = func.utf8_text(source).unwrap_or("").to_owned();
                        if !callee.is_empty() {
                            let line = node.start_position().row + 1;
                            result.edges.push((
                                module_name.to_owned(),
                                callee,
                                Edge::calls(line).with_confidence(
                                    0.7,
                                    graphify_core::types::ConfidenceKind::Inferred,
                                ),
                            ));
                        }
                    }
                    // `field_expression` (method-call on a value) and any
                    // other shape stay out-of-scope: v1 policy per the
                    // FEAT-031 task body's Boundaries section.
                    _ => {}
                }
            }
        }
        "macro_invocation" => {
            extract_macro_invocation(node, source, module_name, result);
        }
        _ => {}
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(&child, source, module_name, local_bindings, result);
    }
}

/// Walk a function/method body once and collect names that locally shadow
/// bare-call resolution (BUG-024). Three sources:
///
/// 1. `let_declaration` patterns (single identifier only — tuple/struct
///    destructuring left for a later pass). Covers closures bound to a name
///    (`let pct = |…| …;`) and let-bound function pointers/values.
/// 2. Nested `function_item` names — `fn sort_key(…) { … }` inside another
///    function body. The extractor's top-level walk only sees root-level
///    items, so nested fns get no `Defines` edge and their callers would
///    otherwise look external.
///
/// Descent skips into nested `function_item` / `impl_item` bodies so a binding
/// inside a nested fn does not leak into the outer function's set (per-scope
/// correctness). The nested fn's NAME is still collected before skipping.
fn collect_local_bindings(body: &tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut bindings = HashSet::new();
    walk_for_bindings(body, source, &mut bindings);
    bindings
}

/// BUG-025: walk a function body for `use_declaration` nodes and register
/// their aliases + Imports edges into `result`. Mirrors `walk_for_bindings`
/// in scope discipline — does not descend into nested `function_item` /
/// `impl_item`, so a `use` inside a nested fn never leaks into the outer
/// scope's alias map.
///
/// Approximation: aliases land in the file-wide `result.use_aliases` map, so
/// in principle a function-scoped `use foo::Bar;` becomes visible to other
/// functions in the same file. In practice the file-wide map already collapses
/// duplicates (last write wins), and same-file shadowing of an aliased name
/// is rare enough that the overcorrection is harmless. A truly per-scope
/// alias map is a future refinement, not a v1 requirement.
fn walk_for_uses(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "use_declaration" => {
            extract_use_declaration(node, source, module_name, result);
            return;
        }
        "function_item" | "impl_item" => {
            // Nested fn / impl have their own lexical scope. Don't descend —
            // their uses (if any) would either be picked up by their own
            // extraction pass (impl) or stay invisible by design (nested fn,
            // tracked separately).
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_uses(&child, source, module_name, result);
    }
}

fn walk_for_bindings(node: &tree_sitter::Node, source: &[u8], bindings: &mut HashSet<String>) {
    match node.kind() {
        "let_declaration" => {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                if pattern.kind() == "identifier" {
                    if let Ok(name) = pattern.utf8_text(source) {
                        if !name.is_empty() {
                            bindings.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        "function_item" => {
            // Collect the name, then DON'T descend — nested fn body has its
            // own scope.
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source) {
                    if !name.is_empty() {
                        bindings.insert(name.to_owned());
                    }
                }
            }
            return;
        }
        "impl_item" => {
            // impl blocks inside a function body are valid Rust but rare and
            // their methods are called as `Type::method()`, not bare. Skip
            // descent for scope hygiene.
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_bindings(&child, source, bindings);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{ConfidenceKind, EdgeKind};

    fn extract(source: &str) -> ExtractionResult {
        let extractor = RustExtractor::new();
        extractor.extract_file(
            Path::new("src/handler.rs"),
            source.as_bytes(),
            "src.handler",
        )
    }

    #[test]
    fn extensions() {
        let e = RustExtractor::new();
        assert_eq!(e.extensions(), &["rs"]);
    }

    #[test]
    fn module_node_always_created() {
        let r = extract("");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].id, "src.handler");
        assert_eq!(r.nodes[0].kind, NodeKind::Module);
        assert_eq!(r.nodes[0].language, Language::Rust);
    }

    #[test]
    fn simple_use_declaration() {
        let r = extract("use std::io;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "std::io");
    }

    #[test]
    fn use_crate_path() {
        let r = extract("use crate::models::user;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "crate::models::user");
    }

    #[test]
    fn grouped_use() {
        let r = extract("use std::{io, fs};\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"std::io"));
        assert!(imports.contains(&"std::fs"));
    }

    #[test]
    fn use_as_alias() {
        let r = extract("use std::io::Result as IoResult;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "std::io::Result");
    }

    #[test]
    fn mod_declaration_file_level() {
        let r = extract("mod handler;\nmod services;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"handler"));
        assert!(imports.contains(&"services"));
    }

    #[test]
    fn inline_mod_no_import_edge() {
        let r = extract("mod inner { pub fn foo() {} }\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert!(
            imports.is_empty(),
            "inline mod should not produce Imports edge"
        );
    }

    #[test]
    fn function_item() {
        let r = extract("pub fn handle_request() -> Result<()> { Ok(()) }\n");
        let funcs: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].id, "src.handler.handle_request");

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "src.handler.handle_request");
    }

    #[test]
    fn struct_item() {
        let r = extract("pub struct Config {\n    port: u16,\n}\n");
        let structs: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Class)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].id, "src.handler.Config");
    }

    #[test]
    fn enum_item() {
        let r = extract("pub enum AppError {\n    NotFound,\n    Internal,\n}\n");
        let enums: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].id, "src.handler.AppError");
    }

    #[test]
    fn trait_item() {
        let r = extract("pub trait Handler {\n    fn handle(&self);\n}\n");
        let traits: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Trait)
            .collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].id, "src.handler.Handler");
    }

    #[test]
    fn impl_block_methods() {
        let r = extract(
            r#"struct Server { port: u16 }
impl Server {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
    pub fn start(&self) {
        listen(self.port);
    }
}
"#,
        );
        let methods: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 2);
        let method_ids: Vec<&str> = methods.iter().map(|m| m.id.as_str()).collect();
        assert!(method_ids.contains(&"src.handler.Server.new"));
        assert!(method_ids.contains(&"src.handler.Server.start"));

        // `listen` is a bare call inside start()
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "listen")
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2.confidence, 0.7);
    }

    #[test]
    fn bare_call_expression() {
        let r = extract("fn main() { process(); }\n");
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence > 0.6)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "process");
        assert_eq!(calls[0].2.confidence, 0.7);
        assert_eq!(calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn method_call_not_extracted_as_bare() {
        let r = extract("fn main() { self.handle(); obj.method(); }\n");
        let bare_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence == 0.7)
            .collect();
        assert!(
            bare_calls.is_empty(),
            "field_expression calls should not produce bare Calls edges"
        );
    }

    #[test]
    fn macro_invocation() {
        let r = extract("fn main() { println!(\"hello\"); vec![1, 2, 3]; }\n");
        let macro_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence < 0.7)
            .collect();
        assert!(macro_calls.len() >= 2);
        let names: Vec<&str> = macro_calls.iter().map(|e| e.1.as_str()).collect();
        assert!(names.contains(&"println"));
        assert!(names.contains(&"vec"));
        assert_eq!(macro_calls[0].2.confidence, 0.6);
        assert_eq!(macro_calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn import_confidence_is_extracted() {
        let r = extract("use std::io;\n");
        let imp = r
            .edges
            .iter()
            .find(|e| e.2.kind == EdgeKind::Imports)
            .unwrap();
        assert_eq!(imp.2.confidence, 1.0);
        assert_eq!(imp.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn full_rust_file() {
        let r = extract(
            r#"use std::io;
use crate::models::User;

pub struct Config {
    host: String,
    port: u16,
}

pub enum AppError {
    NotFound,
    Internal(String),
}

pub trait Handler {
    fn handle(&self) -> Result<(), AppError>;
}

pub fn process(config: &Config) -> Result<(), AppError> {
    validate(config)?;
    Ok(())
}

impl Config {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}
"#,
        );

        // 1 module + 1 struct + 1 enum + 2 enum variants (BUG-027) + 1 trait
        //   + 1 function + 1 method = 8 nodes
        assert_eq!(r.nodes.len(), 8);

        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"std::io"));
        assert!(imports.contains(&"crate::models::User"));

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        // Config, AppError, AppError.NotFound, AppError.Internal, Handler,
        // process, Config.new — BUG-027 added the 2 enum-variant Defines.
        assert_eq!(defines.len(), 7);

        // `validate` is a bare call inside process()
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "validate")
            .collect();
        assert_eq!(calls.len(), 1);
    }

    // -----------------------------------------------------------------------
    // FEAT-031: use_aliases + scoped_identifier call extraction
    // -----------------------------------------------------------------------

    #[test]
    fn feat_031_use_alias_simple_identifier() {
        let r = extract("use std::io;\n");
        assert_eq!(
            r.use_aliases.get("io").map(String::as_str),
            Some("std::io"),
            "short name `io` should alias to full path `std::io`"
        );
    }

    #[test]
    fn feat_031_use_alias_crate_scoped_path() {
        let r = extract("use crate::types::Node;\n");
        assert_eq!(
            r.use_aliases.get("Node").map(String::as_str),
            Some("crate::types::Node"),
            "short name `Node` should alias to `crate::types::Node`"
        );
    }

    #[test]
    fn feat_031_use_alias_grouped_import() {
        let r = extract("use std::{io, fs};\n");
        assert_eq!(r.use_aliases.get("io").map(String::as_str), Some("std::io"));
        assert_eq!(r.use_aliases.get("fs").map(String::as_str), Some("std::fs"));
    }

    #[test]
    fn feat_031_use_alias_grouped_scoped() {
        let r = extract("use crate::types::{Node, Edge};\n");
        assert_eq!(
            r.use_aliases.get("Node").map(String::as_str),
            Some("crate::types::Node")
        );
        assert_eq!(
            r.use_aliases.get("Edge").map(String::as_str),
            Some("crate::types::Edge")
        );
    }

    // -----------------------------------------------------------------------
    // BUG-024: Calls edges must not be emitted for closure bindings or
    // let-bound locals. Per-function scope: a binding in fn A does not
    // shadow a real call in fn B.
    // -----------------------------------------------------------------------

    #[test]
    fn bug_024_closure_binding_skipped() {
        let r =
            extract("fn build() { let pct = |c: usize| -> f64 { c as f64 }; let _ = pct(10); }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            !calls.contains(&"pct"),
            "pct is a closure binding, not a function — must not emit a Calls edge. Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_024_let_binding_skipped_when_called() {
        // Pathological-but-valid pattern surfaced by FEAT-043 dogfood:
        // a let-bound value (function pointer, fn item, FnOnce, etc.) gets
        // called as `name()`. Without scope analysis this looks identical
        // to a bare external call.
        let r = extract("fn build() { let sort_key = some_fn; sort_key(); }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            !calls.contains(&"sort_key"),
            "sort_key is a let-binding, not a function. Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_024_real_external_call_still_emitted() {
        // Regression guard: a real bare external call in a function with
        // unrelated let-bindings must still emit a Calls edge.
        let r = extract("fn build() { let x = 1; foo(); }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            calls.contains(&"foo"),
            "foo() is a genuine bare external call, must remain. Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_024_closure_scope_per_function() {
        // A binding in fn a() must not shadow a bare call in fn b().
        let r = extract("fn a() { let pct = || 0; } fn b() { pct(); }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            calls.contains(&"pct"),
            "pct() in fn b() is bare external (no local binding in scope). Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_024_nested_fn_item_skipped() {
        // Nested `fn sort_key(...)` inside another function body does NOT
        // emit a `Defines` edge (the top-level extractor walk only sees root
        // items), so calls to it would be classified external without scope
        // analysis. Surfaced by FEAT-043 dogfood: `sort_key` in
        // `crates/graphify-core/src/contract.rs::compare_violations`.
        let r =
            extract("fn outer() { fn sort_key(v: &u32) -> u32 { *v } let _ = sort_key(&1); }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            !calls.contains(&"sort_key"),
            "nested fn `sort_key` is local to outer(), must not emit a Calls edge. Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_024_method_body_local_binding_skipped() {
        // Same scope rule applies inside impl method bodies.
        let r =
            extract("struct S; impl S { fn run(&self) { let threshold = 0.5; threshold(); } }\n");
        let calls: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            !calls.contains(&"threshold"),
            "threshold is a let-binding inside the method, must not emit a Calls edge. Got: {:?}",
            calls
        );
    }

    #[test]
    fn bug_023_nested_scoped_use_list_decomposes() {
        // `use a::{b::{c, d}}` must produce 2 imports edges (`a::b::c`,
        // `a::b::d`) and 2 use_aliases entries — not a single edge with
        // literal braces in the target.
        let r = extract("use a::{b::{c, d}};\n");
        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(
            imports,
            vec!["a::b::c", "a::b::d"],
            "nested scoped_use_list must decompose into per-leaf edges"
        );
        assert_eq!(r.use_aliases.get("c").map(String::as_str), Some("a::b::c"));
        assert_eq!(r.use_aliases.get("d").map(String::as_str), Some("a::b::d"));
    }

    #[test]
    fn bug_023_nested_scoped_use_list_mixed_siblings() {
        // BUG-022 dogfood shape: `use foo::{bar::{baz, qux}, other}` —
        // nested group sibling-by-sibling with a flat sibling.
        let r = extract("use foo::{bar::{baz, qux}, other};\n");
        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(
            imports,
            vec!["foo::bar::baz", "foo::bar::qux", "foo::other"]
        );
        assert_eq!(
            r.use_aliases.get("baz").map(String::as_str),
            Some("foo::bar::baz")
        );
        assert_eq!(
            r.use_aliases.get("qux").map(String::as_str),
            Some("foo::bar::qux")
        );
        assert_eq!(
            r.use_aliases.get("other").map(String::as_str),
            Some("foo::other")
        );
    }

    #[test]
    fn feat_031_use_alias_as_clause() {
        let r = extract("use std::io::Result as IoResult;\n");
        // The local short name is the alias (`IoResult`), value is the full path.
        assert_eq!(
            r.use_aliases.get("IoResult").map(String::as_str),
            Some("std::io::Result"),
            "aliased imports should use the local alias as the key"
        );
        // The original short name `Result` should NOT shadow the alias.
        assert!(
            !r.use_aliases.contains_key("Result"),
            "plain `Result` should not be registered when an `as` alias is present"
        );
    }

    #[test]
    fn feat_031_scoped_call_emits_edge() {
        let r = extract(
            "use crate::types::Node;\nfn build() { let _ = Node::module(\"x\", \"\", 0); }\n",
        );
        let scoped_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "Node::module")
            .collect();
        assert_eq!(
            scoped_calls.len(),
            1,
            "scoped_identifier call `Node::module(...)` must emit a Calls edge"
        );
        assert_eq!(scoped_calls[0].2.confidence, 0.7);
        assert_eq!(scoped_calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn feat_031_deep_scoped_call_emits_edge() {
        let r = extract("fn build() { let _ = foo::Bar::baz(); }\n");
        let scoped_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "foo::Bar::baz")
            .collect();
        assert_eq!(
            scoped_calls.len(),
            1,
            "deep scoped_identifier call `foo::Bar::baz()` must emit a Calls edge with full path"
        );
    }

    #[test]
    fn feat_031_scoped_call_in_impl_method_body() {
        let r = extract(
            r#"use crate::types::Node;
struct G;
impl G {
    pub fn add(&self) {
        let _ = Node::symbol("s");
    }
}
"#,
        );
        let scoped_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "Node::symbol")
            .collect();
        assert_eq!(
            scoped_calls.len(),
            1,
            "scoped calls inside impl method bodies must be captured"
        );
    }

    // ----------------------------------------------------------------------
    // BUG-025: function-body use_declaration walking
    // ----------------------------------------------------------------------

    #[test]
    fn bug_025_function_scoped_use_emits_imports_edge() {
        // `use` declarations inside a function body should still produce
        // Imports edges, just like top-level uses.
        let r = extract("fn build() {\n    use foo::Bar;\n}\n");
        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            imports.contains(&"foo::Bar"),
            "function-scoped `use foo::Bar;` must emit an Imports edge. Got: {:?}",
            imports
        );
    }

    #[test]
    fn bug_025_function_scoped_use_registers_alias() {
        // The use_aliases map must be populated so the post-extraction
        // resolver can rewrite `Bar::new()` calls inside the same file.
        let r = extract("fn build() {\n    use foo::Bar;\n}\n");
        assert_eq!(
            r.use_aliases.get("Bar").map(String::as_str),
            Some("foo::Bar"),
            "function-scoped use must populate use_aliases; aliases were: {:?}",
            r.use_aliases
        );
    }

    #[test]
    fn bug_025_function_scoped_grouped_use_decomposes() {
        // Mirrors the BUG-022 dogfood canary: graphify-cli's
        // `apply_suggestions` opens with a grouped function-scoped use.
        let r = extract(
            "fn apply() {\n    use toml_edit::{Array, DocumentMut, Item, Table, Value};\n}\n",
        );
        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        for sym in [
            "toml_edit::Array",
            "toml_edit::DocumentMut",
            "toml_edit::Item",
            "toml_edit::Table",
            "toml_edit::Value",
        ] {
            assert!(
                imports.contains(&sym),
                "function-scoped grouped use missed `{}`. Got: {:?}",
                sym,
                imports
            );
        }
        for short in ["Array", "DocumentMut", "Item", "Table", "Value"] {
            assert!(
                r.use_aliases.contains_key(short),
                "use_aliases missing `{}`; got: {:?}",
                short,
                r.use_aliases
            );
        }
    }

    #[test]
    fn bug_025_method_scoped_use_registers_alias() {
        // Same fix must reach impl-method bodies.
        let r =
            extract("struct G;\nimpl G {\n    fn run(&self) {\n        use foo::Bar;\n    }\n}\n");
        assert_eq!(
            r.use_aliases.get("Bar").map(String::as_str),
            Some("foo::Bar"),
            "impl-method-scoped use must populate use_aliases; aliases were: {:?}",
            r.use_aliases
        );
    }

    // -----------------------------------------------------------------------
    // FEAT-045: pub use → ReExportEntry emission
    // -----------------------------------------------------------------------

    #[test]
    fn feat_045_pub_use_simple_emits_reexport() {
        let r = extract("pub use foo::bar::Baz;\n");
        assert_eq!(r.reexports.len(), 1, "one ReExportEntry expected");
        let entry = &r.reexports[0];
        assert_eq!(entry.from_module, "src.handler");
        assert_eq!(entry.raw_target, "foo::bar");
        assert_eq!(entry.line, 1);
        assert!(!entry.is_star);
        assert_eq!(entry.specs.len(), 1);
        assert_eq!(entry.specs[0].exported_name, "Baz");
        assert_eq!(entry.specs[0].local_name, "Baz");
    }

    #[test]
    fn feat_045_pub_use_aliased_carries_local_name() {
        let r = extract("pub use foo::bar::Baz as Qux;\n");
        assert_eq!(r.reexports.len(), 1);
        let entry = &r.reexports[0];
        assert_eq!(entry.raw_target, "foo::bar");
        assert_eq!(entry.specs.len(), 1);
        assert_eq!(entry.specs[0].exported_name, "Baz");
        assert_eq!(entry.specs[0].local_name, "Qux");
    }

    #[test]
    fn feat_045_pub_use_grouped_emits_one_entry_per_target() {
        let r = extract("pub use foo::{Bar, Baz};\n");
        assert_eq!(
            r.reexports.len(),
            1,
            "single shared raw_target collapses to one entry; got: {:?}",
            r.reexports
        );
        let entry = &r.reexports[0];
        assert_eq!(entry.raw_target, "foo");
        let names: Vec<(&str, &str)> = entry
            .specs
            .iter()
            .map(|s| (s.exported_name.as_str(), s.local_name.as_str()))
            .collect();
        assert_eq!(names, vec![("Bar", "Bar"), ("Baz", "Baz")]);
    }

    #[test]
    fn feat_045_pub_use_nested_grouped_buckets_by_prefix() {
        // `pub use foo::{bar::{Baz, Qux}};` — nested group expands to two
        // leaves under combined prefix `foo::bar`. One bucket, one entry.
        let r = extract("pub use foo::{bar::{Baz, Qux}};\n");
        assert_eq!(r.reexports.len(), 1, "one bucket expected");
        let entry = &r.reexports[0];
        assert_eq!(entry.raw_target, "foo::bar");
        let names: Vec<&str> = entry
            .specs
            .iter()
            .map(|s| s.exported_name.as_str())
            .collect();
        assert_eq!(names, vec!["Baz", "Qux"]);

        // And a mixed shape: `pub use foo::{bar::Baz, Qux};` — two buckets,
        // one for `foo::bar`, one for `foo`.
        let r2 = extract("pub use foo::{bar::Baz, Qux};\n");
        assert_eq!(r2.reexports.len(), 2);
        let by_target: std::collections::HashMap<_, _> = r2
            .reexports
            .iter()
            .map(|e| (e.raw_target.as_str(), &e.specs))
            .collect();
        assert_eq!(by_target.get("foo::bar").map(|s| s.len()), Some(1));
        assert_eq!(by_target.get("foo").map(|s| s.len()), Some(1));
        assert_eq!(by_target.get("foo::bar").unwrap()[0].exported_name, "Baz");
        assert_eq!(by_target.get("foo").unwrap()[0].exported_name, "Qux");
    }

    #[test]
    fn feat_045_pub_use_intra_crate_canonical_chain_shape() {
        // Mirrors a barrel's view: `lib.rs` re-exports a sibling module's
        // type. Confirms the entry shape downstream FEAT-046/047 will feed
        // into ReExportGraph::build().
        let r = extract("pub use crate::types::Node;\npub use crate::graph::CodeGraph;\n");
        assert_eq!(r.reexports.len(), 2);

        let by_target: std::collections::HashMap<_, _> = r
            .reexports
            .iter()
            .map(|e| (e.raw_target.as_str(), &e.specs))
            .collect();
        assert_eq!(
            by_target
                .get("crate::types")
                .map(|s| s[0].exported_name.as_str()),
            Some("Node")
        );
        assert_eq!(
            by_target
                .get("crate::graph")
                .map(|s| s[0].exported_name.as_str()),
            Some("CodeGraph")
        );

        // Sanity: existing Imports + use_aliases pipeline is untouched
        // (ReExportEntry is additive in FEAT-045).
        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert!(imports.contains(&"crate::types::Node"));
        assert!(imports.contains(&"crate::graph::CodeGraph"));
        assert_eq!(
            r.use_aliases.get("Node").map(String::as_str),
            Some("crate::types::Node")
        );
    }

    #[test]
    fn feat_045_plain_use_does_not_emit_reexport() {
        // Visibility check: plain `use` (private) must NOT produce a
        // ReExportEntry — nothing is being re-exported from this module.
        let r = extract("use foo::bar::Baz;\n");
        assert!(
            r.reexports.is_empty(),
            "plain `use` must not emit ReExportEntry; got: {:?}",
            r.reexports
        );
    }

    #[test]
    fn feat_045_pub_crate_use_still_emits_reexport() {
        // Restricted-pub `pub(crate) use` IS still a re-export within the
        // crate (Rust semantics). Canonical-collapse needs the entry so
        // intra-crate barrel chains can be walked. Wildcards remain OOS.
        let r = extract("pub(crate) use foo::bar::Baz;\n");
        assert_eq!(r.reexports.len(), 1);
        assert_eq!(r.reexports[0].raw_target, "foo::bar");
        assert_eq!(r.reexports[0].specs[0].exported_name, "Baz");
    }

    #[test]
    fn bug_025_nested_fn_use_does_not_leak_to_outer() {
        // Lexical scope hygiene: a `use` inside a nested fn must NOT register
        // an alias visible to the outer function. Per-scope correctness mirrors
        // BUG-024's nested-fn binding handling.
        let r = extract("fn outer() {\n    fn inner() {\n        use foo::OnlyInner;\n    }\n}\n");
        assert!(
            !r.use_aliases.contains_key("OnlyInner"),
            "nested-fn use must not leak into outer scope; aliases were: {:?}",
            r.use_aliases
        );
    }

    // ----- FEAT-049: pub type alias collapse (Defines edge for type_item) ----

    #[test]
    fn feat_049_pub_type_alias_emits_defines_edge() {
        // `pub type Cycle = Vec<String>;` is the dogfood case from
        // graphify-report. Without the extractor handler, `Cycle` never lands
        // in `known_modules` and consumer `use crate::Cycle;` references
        // resolve to a non-local placeholder, surfacing in `suggest stubs`.
        let r = extract("pub type Cycle = Vec<String>;\n");
        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "src.handler.Cycle")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected one Defines edge to src.handler.Cycle; edges: {:?}",
            r.edges
        );
        let symbol = r
            .nodes
            .iter()
            .find(|n| n.id == "src.handler.Cycle")
            .expect("type alias symbol node missing");
        assert_eq!(symbol.kind, NodeKind::Class);
        assert!(symbol.is_local);
    }

    #[test]
    fn feat_049_private_type_alias_also_emits_defines_edge() {
        // Non-pub `type X = Y;` should still register the local symbol — same
        // shape as struct/enum/trait, which never gate on visibility. Keeps
        // intra-crate references resolvable when the alias is consumed via
        // sibling-module access patterns.
        let r = extract("type ShortAlias = u64;\n");
        let symbol = r.nodes.iter().find(|n| n.id == "src.handler.ShortAlias");
        assert!(
            symbol.is_some(),
            "private type alias must still register a Defines edge; nodes: {:?}",
            r.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn feat_049_scoped_rhs_type_alias_still_registers_lhs() {
        // RHS shape (`scoped_type_identifier`) must not affect LHS extraction;
        // canonical-collapse of the RHS is deliberately out of scope.
        let r = extract("pub type Foo = other::Bar;\n");
        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "src.handler.Foo")
            .collect();
        assert_eq!(defines.len(), 1, "edges: {:?}", r.edges);
    }

    // ----- BUG-027: Defines emission for static_item, const_item, enum variants

    #[test]
    fn bug_027_pub_static_item_emits_defines_edge() {
        // `pub static INTEGRATIONS: Dir<'_> = include_dir!(...);` is the
        // dogfood case from graphify-cli. Without the extractor handler, the
        // canonical id never lands in `known_modules` and consumer
        // `use crate::install::copy_plan::INTEGRATIONS;` references resolve
        // to a non-local placeholder, surfacing in `suggest stubs`.
        let r = extract("pub static INTEGRATIONS: u32 = 1;\n");
        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "src.handler.INTEGRATIONS")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected one Defines edge to src.handler.INTEGRATIONS; edges: {:?}",
            r.edges
        );
        let symbol = r
            .nodes
            .iter()
            .find(|n| n.id == "src.handler.INTEGRATIONS")
            .expect("static symbol node missing");
        assert!(symbol.is_local);
    }

    #[test]
    fn bug_027_private_static_item_also_emits_defines_edge() {
        // Non-pub `static X = …;` must still register the local symbol —
        // visibility never gates Defines emission for any other item kind.
        let r = extract("static LOCAL_CACHE: u32 = 0;\n");
        let symbol = r.nodes.iter().find(|n| n.id == "src.handler.LOCAL_CACHE");
        assert!(
            symbol.is_some(),
            "private static must still register a Defines edge; nodes: {:?}",
            r.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn bug_027_const_item_emits_defines_edge() {
        // `const_item` shares the same fix shape as `static_item` — both have
        // a `name` field of `identifier` type. Folded in for atomicity so the
        // gap doesn't reappear later via a `pub const FOO …` callsite.
        let r = extract("pub const MAX_RETRIES: u32 = 3;\n");
        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "src.handler.MAX_RETRIES")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected one Defines edge to src.handler.MAX_RETRIES; edges: {:?}",
            r.edges
        );
    }

    #[test]
    fn bug_027_enum_variants_emit_defines_edges() {
        // `enum Selector { Project(String), Group(String) }` is the dogfood
        // case from graphify-core/policy.rs. The extractor today emits Defines
        // only for the enum itself; bare `Selector::Group(...)` callsites
        // synthesize `src.handler.Selector.Group` (resolver case 8.6) and miss
        // `known_modules` because no Defines edge ever registered the variant.
        let r = extract("enum Selector {\n    Project(String),\n    Group(String),\n}\n");
        let variant_targets: Vec<&str> = r
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .map(|(_, t, _)| t.as_str())
            .filter(|t| t.starts_with("src.handler.Selector."))
            .collect();
        assert!(
            variant_targets.contains(&"src.handler.Selector.Project"),
            "missing Defines for Selector::Project; targets seen: {:?}",
            variant_targets
        );
        assert!(
            variant_targets.contains(&"src.handler.Selector.Group"),
            "missing Defines for Selector::Group; targets seen: {:?}",
            variant_targets
        );
    }

    #[test]
    fn bug_027_unit_enum_variants_also_emit_defines_edges() {
        // Unit variants (no payload) follow the same shape — the variant
        // `name` field is present regardless of whether the variant has tuple
        // or struct fields.
        let r = extract("pub enum Status {\n    Open,\n    Closed,\n}\n");
        let variant_targets: Vec<&str> = r
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .map(|(_, t, _)| t.as_str())
            .filter(|t| t.starts_with("src.handler.Status."))
            .collect();
        assert!(variant_targets.contains(&"src.handler.Status.Open"));
        assert!(variant_targets.contains(&"src.handler.Status.Closed"));
    }
}
