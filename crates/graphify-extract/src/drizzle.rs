use std::path::PathBuf;

use graphify_core::contract::{Contract, ContractSide, Field, FieldType, PrimitiveType};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq)]
pub struct DrizzleParseError {
    pub message: String,
}

impl std::fmt::Display for DrizzleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for DrizzleParseError {}

pub fn extract_drizzle_contract(source: &str, table: &str) -> Result<Contract, DrizzleParseError> {
    extract_drizzle_contract_at(source, table, PathBuf::from("<inline>"))
}

pub fn extract_drizzle_contract_at(
    source: &str,
    table: &str,
    source_file: PathBuf,
) -> Result<Contract, DrizzleParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| DrizzleParseError {
            message: format!("load TS grammar: {e}"),
        })?;

    let tree = parser.parse(source, None).ok_or_else(|| DrizzleParseError {
        message: "TS parse returned None".into(),
    })?;

    let bytes = source.as_bytes();
    let mut found: Option<(Node<'_>, &str)> = None;

    // Walk every `export_statement` → `lexical_declaration` → variable_declarator
    // whose value is a call to one of the known table constructors.
    walk_table_bindings(tree.root_node(), bytes, &mut |decl_name, call_node, call_name| {
        if (call_name.ends_with("Table") || is_schema_table_chain(call_node, bytes))
            && first_string_arg(call_node, bytes).as_deref() == Some(table)
        {
            found = Some((call_node, decl_name));
        }
    });

    let Some((call_node, _decl_name)) = found else {
        return Err(DrizzleParseError {
            message: format!("table '{table}' not found in source"),
        });
    };

    let cols_node = call_node
        .child_by_field_name("arguments")
        .and_then(|args| nth_argument(args, 1, bytes))
        .ok_or_else(|| DrizzleParseError {
            message: "table call is missing a columns object literal".into(),
        })?;

    let fields = parse_columns_object(cols_node, bytes)?;

    let _table_line = call_node.start_position().row + 1;

    Ok(Contract {
        name: table.to_string(),
        side: ContractSide::Orm,
        source_file,
        source_symbol: table.to_string(),
        fields,
        relations: Vec::new(),
        // NOTE: `table_line` not stored on Contract; pair-level line is tracked in CLI output layer.
    })
}

fn walk_table_bindings<'a, F>(node: Node<'a>, bytes: &'a [u8], on_match: &mut F)
where
    F: FnMut(&'a str, Node<'a>, &'a str),
{
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "export_statement" || child.kind() == "lexical_declaration" {
            for decl in child.children(&mut child.walk()) {
                if decl.kind() == "variable_declarator" {
                    if let (Some(name_node), Some(value_node)) = (
                        decl.child_by_field_name("name"),
                        decl.child_by_field_name("value"),
                    ) {
                        if value_node.kind() == "call_expression" {
                            if let Some((callee, callee_name)) = callee_name_of(value_node, bytes) {
                                let _ = callee; // retained for potential future use
                                let decl_name = text_of(name_node, bytes);
                                on_match(decl_name, value_node, callee_name);
                            }
                        }
                    }
                }
            }
        }
        walk_table_bindings(child, bytes, on_match);
    }
}

fn callee_name_of<'a>(call: Node<'a>, bytes: &'a [u8]) -> Option<(Node<'a>, &'a str)> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some((func, text_of(func, bytes))),
        "member_expression" => {
            let property = func.child_by_field_name("property")?;
            Some((property, text_of(property, bytes)))
        }
        _ => None,
    }
}

fn is_schema_table_chain(call: Node<'_>, bytes: &[u8]) -> bool {
    // Matches `pgSchema('auth').table(...)` — the call's `function` is a
    // member_expression whose `.object` is itself a call_expression named `pgSchema`.
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let property = func
        .child_by_field_name("property")
        .map(|p| text_of(p, bytes));
    if property != Some("table") {
        return false;
    }
    let Some(object) = func.child_by_field_name("object") else {
        return false;
    };
    if object.kind() != "call_expression" {
        return false;
    }
    callee_name_of(object, bytes)
        .map(|(_, n)| n == "pgSchema" || n == "mysqlSchema" || n == "sqliteSchema")
        .unwrap_or(false)
}

fn nth_argument<'a>(args: Node<'a>, n: usize, _bytes: &'a [u8]) -> Option<Node<'a>> {
    let mut cursor = args.walk();
    for (idx, child) in args.named_children(&mut cursor).enumerate() {
        if idx == n {
            return Some(child);
        }
    }
    None
}

fn first_string_arg(call: Node<'_>, bytes: &[u8]) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() == "string" {
            return Some(string_literal_value(child, bytes));
        }
    }
    None
}

fn string_literal_value(node: Node<'_>, bytes: &[u8]) -> String {
    let raw = text_of(node, bytes);
    raw.trim_matches(|c| c == '\'' || c == '"' || c == '`').to_string()
}

fn text_of<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn parse_columns_object(obj: Node<'_>, bytes: &[u8]) -> Result<Vec<Field>, DrizzleParseError> {
    if obj.kind() != "object" {
        return Err(DrizzleParseError {
            message: format!("expected object literal, got {}", obj.kind()),
        });
    }
    let mut fields = Vec::new();
    let mut cursor = obj.walk();
    for pair in obj.named_children(&mut cursor) {
        if pair.kind() != "pair" {
            // spread_element and others — skip with a best-effort warning in stderr
            if pair.kind() == "spread_element" {
                eprintln!("drizzle: spread in column object is not expanded in v1");
            }
            continue;
        }
        let key_node = pair
            .child_by_field_name("key")
            .ok_or_else(|| DrizzleParseError { message: "pair missing key".into() })?;
        let value_node = pair
            .child_by_field_name("value")
            .ok_or_else(|| DrizzleParseError { message: "pair missing value".into() })?;
        let raw_name = property_key_text(key_node, bytes).to_string();
        let (type_ref, nullable, has_default) = interpret_column_chain(value_node, bytes);
        let line = pair.start_position().row + 1;
        fields.push(Field {
            name: raw_name.clone(),
            raw_name,
            type_ref,
            nullable,
            has_default,
            line,
        });
    }
    Ok(fields)
}

fn property_key_text<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    match node.kind() {
        "property_identifier" | "identifier" => text_of(node, bytes),
        "string" => {
            let raw = text_of(node, bytes);
            raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
        }
        _ => text_of(node, bytes),
    }
}

fn interpret_column_chain(value: Node<'_>, bytes: &[u8]) -> (FieldType, bool, bool) {
    // Walk chain leftwards collecting method names; the leftmost call is the builder.
    let mut chain_calls: Vec<&str> = Vec::new();
    let mut current = value;
    let mut root_builder: Option<Node<'_>> = None;

    while current.kind() == "call_expression" {
        let Some((callee_node, callee_name)) = callee_name_of(current, bytes) else {
            break;
        };
        let Some(callee_func) = current.child_by_field_name("function") else {
            break;
        };
        if callee_func.kind() == "member_expression" {
            chain_calls.push(callee_name);
            let Some(receiver) = callee_func.child_by_field_name("object") else {
                break;
            };
            current = receiver;
        } else {
            root_builder = Some(callee_node);
            break;
        }
    }

    let builder_name = root_builder.map(|n| text_of(n, bytes)).unwrap_or("");
    let nullable = !chain_calls.contains(&"notNull");
    let has_default = chain_calls
        .iter()
        .any(|c| c.starts_with("default") || *c == "$default");

    // `.$type<Foo>()` — if present, override the type to Named("Foo").
    let dollar_type = chain_calls.contains(&"$type");

    let primitive = match builder_name {
        "text" | "varchar" | "char" | "uuid" => Some(PrimitiveType::String),
        "integer" | "serial" | "bigserial" | "smallint" | "real" | "double_precision"
        | "numeric" | "decimal" => Some(PrimitiveType::Number),
        "boolean" => Some(PrimitiveType::Boolean),
        "timestamp" | "date" | "time" => Some(PrimitiveType::Date),
        "json" | "jsonb" => Some(PrimitiveType::Unknown),
        _ => None,
    };

    let type_ref = match (primitive, dollar_type) {
        (_, true) => {
            // Extract the generic argument text from the chain by scanning value source.
            // For v1, store the name heuristically: parse from the full source range.
            FieldType::Named {
                value: extract_dollar_type_arg(value, bytes).unwrap_or_else(|| "unknown".into()),
            }
        }
        (Some(PrimitiveType::Unknown), false) => FieldType::Primitive {
            value: PrimitiveType::Unknown,
        },
        (Some(p), false) => FieldType::Primitive { value: p },
        (None, false) => FieldType::Unmapped {
            value: builder_name.to_string(),
        },
    };

    (type_ref, nullable, has_default)
}

fn extract_dollar_type_arg(value: Node<'_>, bytes: &[u8]) -> Option<String> {
    // Search textually: `.$type<Foo>()` — simpler than threading through the AST
    // because tree-sitter-typescript splits type arguments into `type_arguments` nodes
    // only in a .ts grammar and not reliably for all chain forms.
    let src = text_of(value, bytes);
    let start = src.find("$type<")?;
    let after = &src[start + "$type<".len()..];
    let end = after.find('>')?;
    Some(after[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::contract::PrimitiveType::*;

    fn assert_field(c: &Contract, name: &str, expected: FieldType, nullable: bool) {
        let f = c
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("missing field {name}"));
        assert_eq!(f.type_ref, expected, "wrong type for {name}");
        assert_eq!(f.nullable, nullable, "wrong nullability for {name}");
    }

    #[test]
    fn parses_scalar_pg_table() {
        let src = r#"
import { pgTable, text, integer, uuid, timestamp, boolean } from 'drizzle-orm/pg-core';
export const users = pgTable('users', {
  id:        uuid('id').primaryKey().defaultRandom(),
  email:     text('email').notNull(),
  age:       integer('age'),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  active:    boolean('active').notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "users").expect("parse ok");
        assert_eq!(c.side, ContractSide::Orm);
        assert_eq!(c.name, "users");
        assert_eq!(c.fields.len(), 5);
        assert_field(&c, "id",        FieldType::Primitive { value: String },  true);  // no .notNull()
        assert_field(&c, "email",     FieldType::Primitive { value: String },  false);
        assert_field(&c, "age",       FieldType::Primitive { value: Number },  true);
        assert_field(&c, "createdAt", FieldType::Primitive { value: Date },    false);
        assert_field(&c, "active",    FieldType::Primitive { value: Boolean }, false);
    }

    #[test]
    fn parses_sqlite_and_mysql_tables() {
        let sqlite = r#"
import { sqliteTable, text, integer } from 'drizzle-orm/sqlite-core';
export const todos = sqliteTable('todos', {
  id:   integer('id').primaryKey(),
  body: text('body').notNull(),
});
"#;
        let c = extract_drizzle_contract(sqlite, "todos").expect("sqlite ok");
        assert_eq!(c.fields.len(), 2);

        let mysql = r#"
import { mysqlTable, varchar, int } from 'drizzle-orm/mysql-core';
export const items = mysqlTable('items', {
  id:   int('id').primaryKey(),
  name: varchar('name', { length: 255 }).notNull(),
});
"#;
        // `int` is not in our default map → Unmapped. `varchar` is String.
        let c = extract_drizzle_contract(mysql, "items").expect("mysql ok");
        assert_field(&c, "name", FieldType::Primitive { value: String }, false);
        assert!(matches!(
            c.fields.iter().find(|f| f.name == "id").unwrap().type_ref,
            FieldType::Unmapped { .. }
        ));
    }

    #[test]
    fn unknown_type_is_unmapped() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core';
export const posts = pgTable('posts', {
  tags: tsvector('tags').notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "posts").expect("ok");
        let f = c.fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(matches!(&f.type_ref, FieldType::Unmapped { value } if value == "tsvector"));
    }
}
