use std::path::PathBuf;

use graphify_core::contract::{
    Cardinality, Contract, ContractSide, Field, FieldType, PrimitiveType, Relation,
};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq)]
pub struct TsContractParseError {
    pub message: String,
}

impl std::fmt::Display for TsContractParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TsContractParseError {}

pub fn extract_ts_contract(source: &str, export: &str) -> Result<Contract, TsContractParseError> {
    extract_ts_contract_at(source, export, PathBuf::from("<inline>"))
}

pub fn extract_ts_contract_at(
    source: &str,
    export: &str,
    source_file: PathBuf,
) -> Result<Contract, TsContractParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| TsContractParseError {
            message: format!("load TS grammar: {e}"),
        })?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| TsContractParseError {
            message: "TS parse returned None".into(),
        })?;
    let bytes = source.as_bytes();

    let target = find_declaration(tree.root_node(), bytes, export).ok_or_else(|| {
        TsContractParseError {
            message: format!("export '{export}' not found"),
        }
    })?;

    let (fields, relations) = match target.kind() {
        "interface_declaration" => parse_interface(target, bytes)?,
        "type_alias_declaration" => parse_type_alias(target, bytes)?,
        other => {
            return Err(TsContractParseError {
                message: format!("unsupported declaration kind: {other}"),
            })
        }
    };

    Ok(Contract {
        name: export.to_string(),
        side: ContractSide::Ts,
        source_file,
        source_symbol: export.to_string(),
        fields,
        relations,
    })
}

fn find_declaration<'a>(root: Node<'a>, bytes: &'a [u8], export: &str) -> Option<Node<'a>> {
    let mut found = None;
    walk_declarations(root, bytes, &mut |kind, name, node| {
        if name == export
            && (kind == "interface_declaration" || kind == "type_alias_declaration")
            && found.is_none()
        {
            found = Some(node);
        }
    });
    found
}

fn walk_declarations<'a, F>(node: Node<'a>, bytes: &'a [u8], on_decl: &mut F)
where
    F: FnMut(&str, &str, Node<'a>),
{
    if matches!(
        node.kind(),
        "interface_declaration" | "type_alias_declaration"
    ) {
        if let Some(name_node) = node.child_by_field_name("name") {
            on_decl(node.kind(), text_of(name_node, bytes), node);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_declarations(child, bytes, on_decl);
    }
}

fn text_of<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn parse_interface(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let body = node
        .child_by_field_name("body")
        .ok_or_else(|| TsContractParseError {
            message: "interface missing body".into(),
        })?;
    parse_members(body, bytes)
}

fn parse_type_alias(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let value = node
        .child_by_field_name("value")
        .ok_or_else(|| TsContractParseError {
            message: "type alias missing value".into(),
        })?;
    let target = match value.kind() {
        "object_type" => value,
        "intersection_type" => {
            // flatten members; see Task 11.
            return parse_intersection(value, bytes);
        }
        other => {
            return Err(TsContractParseError {
                message: format!("unsupported type alias value: {other}"),
            })
        }
    };
    parse_members(target, bytes)
}

fn parse_members(
    body: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let mut fields = Vec::new();
    let relations: Vec<Relation> = Vec::new(); // relations classification done in Task 10

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "property_signature" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else {
            continue;
        };
        let raw_name = property_name_text(name_node, bytes).to_string();
        let mut child_cursor = member.walk();
        let optional = member
            .children(&mut child_cursor)
            .any(|n| n.kind() == "?");
        let type_node = member.child_by_field_name("type");
        let (type_ref, mut nullable) = match type_node {
            Some(t) => resolve_type_annotation(t, bytes),
            None => (
                FieldType::Primitive {
                    value: PrimitiveType::Unknown,
                },
                false,
            ),
        };
        if optional {
            nullable = true;
        }
        let line = member.start_position().row + 1;
        fields.push(Field {
            name: raw_name.clone(),
            raw_name,
            type_ref,
            nullable,
            has_default: false,
            line,
        });
    }

    Ok((fields, relations))
}

fn parse_intersection(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    // Covered in Task 11.
    let mut fields: Vec<Field> = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "object_type" {
            let (members, _) = parse_members(child, bytes)?;
            for f in members {
                if let Some(existing) = fields.iter_mut().find(|x| x.name == f.name) {
                    *existing = f;
                } else {
                    fields.push(f);
                }
            }
        } else {
            eprintln!(
                "ts_contract: intersection with non-inline type '{}' ignored in v1",
                child.kind()
            );
        }
    }
    Ok((fields, Vec::new()))
}

fn property_name_text<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    match node.kind() {
        "property_identifier" | "identifier" => text_of(node, bytes),
        "string" => text_of(node, bytes).trim_matches(|c: char| c == '\'' || c == '"' || c == '`'),
        _ => text_of(node, bytes),
    }
}

/// Resolve a `type_annotation` or direct type node to a (FieldType, nullable) pair.
fn resolve_type_annotation(ann: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    // A type_annotation wraps the actual type as its first named child.
    let inner = if ann.kind() == "type_annotation" {
        let mut cursor = ann.walk();
        let first = ann.named_children(&mut cursor).next();
        first.unwrap_or(ann)
    } else {
        ann
    };
    resolve_type(inner, bytes)
}

fn resolve_type(node: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    match node.kind() {
        "predefined_type" => match text_of(node, bytes) {
            "string" => (prim(PrimitiveType::String), false),
            "number" | "bigint" => (prim(PrimitiveType::Number), false),
            "boolean" => (prim(PrimitiveType::Boolean), false),
            "unknown" | "any" | "never" | "void" => (prim(PrimitiveType::Unknown), false),
            other => (
                FieldType::Named {
                    value: other.to_string(),
                },
                false,
            ),
        },
        "type_identifier" => {
            let name = text_of(node, bytes);
            if name == "Date" {
                (prim(PrimitiveType::Date), false)
            } else {
                (
                    FieldType::Named {
                        value: name.to_string(),
                    },
                    false,
                )
            }
        }
        "literal_type" => {
            let src = text_of(node, bytes).trim();
            if src == "null" || src == "undefined" {
                (prim(PrimitiveType::Unknown), true)
            } else {
                (
                    FieldType::Named {
                        value: src.to_string(),
                    },
                    false,
                )
            }
        }
        "null" | "undefined" => (prim(PrimitiveType::Unknown), true),
        "union_type" => resolve_union(node, bytes),
        "array_type" => {
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            let (inner_ty, inner_nullable) = inner
                .map(|n| resolve_type(n, bytes))
                .unwrap_or((prim(PrimitiveType::Unknown), false));
            (
                FieldType::Array {
                    value: Box::new(inner_ty),
                },
                inner_nullable,
            )
        }
        "generic_type" => {
            // Handle `Array<T>` explicitly.
            let name_node = node.child_by_field_name("name");
            let name = name_node.map(|n| text_of(n, bytes)).unwrap_or("");
            if name == "Array" {
                let args = node.child_by_field_name("type_arguments");
                let mut cursor = args.map(|a| a.walk()).unwrap_or_else(|| node.walk());
                let inner = args.and_then(|a| a.named_children(&mut cursor).next());
                let (inner_ty, _) = inner
                    .map(|n| resolve_type(n, bytes))
                    .unwrap_or((prim(PrimitiveType::Unknown), false));
                (
                    FieldType::Array {
                        value: Box::new(inner_ty),
                    },
                    false,
                )
            } else {
                (
                    FieldType::Named {
                        value: name.to_string(),
                    },
                    false,
                )
            }
        }
        "parenthesized_type" => {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            first
                .map(|n| resolve_type(n, bytes))
                .unwrap_or((prim(PrimitiveType::Unknown), false))
        }
        _ => (
            FieldType::Named {
                value: text_of(node, bytes).to_string(),
            },
            false,
        ),
    }
}

fn resolve_union(node: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    let mut parts: Vec<FieldType> = Vec::new();
    let mut nullable = false;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let (t, n) = resolve_type(child, bytes);
        if n {
            nullable = true;
            continue;
        }
        match &t {
            FieldType::Primitive {
                value: PrimitiveType::Unknown,
            } => {
                // null/undefined literals are represented as Unknown + nullable;
                // once we've already extracted nullable=true we skip them from the union.
                continue;
            }
            _ => parts.push(t),
        }
    }
    let ty = if parts.len() == 1 {
        parts.pop().unwrap()
    } else if parts.is_empty() {
        FieldType::Primitive {
            value: PrimitiveType::Unknown,
        }
    } else {
        FieldType::Union { value: parts }
    };
    (ty, nullable)
}

fn prim(p: PrimitiveType) -> FieldType {
    FieldType::Primitive { value: p }
}

/// Parse multiple TS contracts from a single source and reclassify fields
/// that reference other known contracts as relations.
pub fn parse_all_ts_contracts(
    source: &str,
    exports: &[&str],
) -> Result<Vec<Contract>, TsContractParseError> {
    parse_all_ts_contracts_at(source, exports, PathBuf::from("<inline>"))
}

pub fn parse_all_ts_contracts_at(
    source: &str,
    exports: &[&str],
    source_file: PathBuf,
) -> Result<Vec<Contract>, TsContractParseError> {
    let mut contracts = Vec::with_capacity(exports.len());
    for export in exports {
        contracts.push(extract_ts_contract_at(source, export, source_file.clone())?);
    }
    let known: std::collections::HashSet<String> =
        contracts.iter().map(|c| c.name.clone()).collect();
    for c in &mut contracts {
        classify_relations(c, &known);
    }
    Ok(contracts)
}

fn classify_relations(contract: &mut Contract, known: &std::collections::HashSet<String>) {
    let mut i = 0;
    while i < contract.fields.len() {
        let f = &contract.fields[i];
        let relation = match &f.type_ref {
            FieldType::Named { value } if known.contains(value) => {
                Some((value.clone(), Cardinality::One))
            }
            FieldType::Array { value } => match value.as_ref() {
                FieldType::Named { value: inner } if known.contains(inner) => {
                    Some((inner.clone(), Cardinality::Many))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((target, cardinality)) = relation {
            let f = contract.fields.remove(i);
            contract.relations.push(Relation {
                name: f.name,
                raw_name: f.raw_name,
                cardinality,
                target_contract: target,
                nullable: f.nullable,
                line: f.line,
            });
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interface_primitive_fields() {
        let src = r#"
export interface UserDto {
  id: string;
  email: string;
  age: number | null;
  active: boolean;
  createdAt: Date;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        assert_eq!(c.side, ContractSide::Ts);
        assert_eq!(c.fields.len(), 5);
        assert!(matches!(
            c.fields.iter().find(|f| f.name == "id").unwrap().type_ref,
            FieldType::Primitive {
                value: PrimitiveType::String
            }
        ));
        let age = c.fields.iter().find(|f| f.name == "age").unwrap();
        assert!(age.nullable);
        let created = c.fields.iter().find(|f| f.name == "createdAt").unwrap();
        assert!(matches!(
            created.type_ref,
            FieldType::Primitive {
                value: PrimitiveType::Date
            }
        ));
    }

    #[test]
    fn parses_type_alias_object() {
        let src = r#"
export type UserDto = {
  id: string;
  nickname?: string;
};
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        assert_eq!(c.fields.len(), 2);
        let nick = c.fields.iter().find(|f| f.name == "nickname").unwrap();
        assert!(nick.nullable);
    }

    #[test]
    fn parses_array_and_generic_array() {
        let src = r#"
export interface UserDto {
  names: string[];
  tags: Array<string>;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        let names = c.fields.iter().find(|f| f.name == "names").unwrap();
        assert!(matches!(
            &names.type_ref,
            FieldType::Array { value } if matches!(**value, FieldType::Primitive { value: PrimitiveType::String })
        ));
        let tags = c.fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(matches!(
            &tags.type_ref,
            FieldType::Array { value } if matches!(**value, FieldType::Primitive { value: PrimitiveType::String })
        ));
    }

    #[test]
    fn collapses_null_and_undefined_into_nullable() {
        let src = r#"
export interface UserDto {
  a: string | null;
  b: string | undefined;
  c?: string;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        for f in &c.fields {
            assert!(f.nullable, "{} expected nullable", f.name);
            assert!(matches!(
                f.type_ref,
                FieldType::Primitive {
                    value: PrimitiveType::String
                }
            ));
        }
    }

    #[test]
    fn classifies_single_and_many_relations() {
        let src = r#"
export interface ProfileDto { id: string }
export interface PostDto { id: string }
export interface UserDto {
  id: string;
  profile?: ProfileDto;
  posts: PostDto[];
}
"#;
        // One-shot helper: parse all three, then reclassify UserDto.
        let contracts = parse_all_ts_contracts(src, &["UserDto", "ProfileDto", "PostDto"]).expect("ok");
        let user = contracts.iter().find(|c| c.name == "UserDto").unwrap();
        assert_eq!(user.relations.len(), 2);
        let profile = user.relations.iter().find(|r| r.name == "profile").unwrap();
        assert_eq!(profile.cardinality, Cardinality::One);
        assert!(profile.nullable);
        let posts = user.relations.iter().find(|r| r.name == "posts").unwrap();
        assert_eq!(posts.cardinality, Cardinality::Many);
        // Scalar-only fields stay in fields[].
        assert_eq!(user.fields.len(), 1);
        assert_eq!(user.fields[0].name, "id");
    }
}
