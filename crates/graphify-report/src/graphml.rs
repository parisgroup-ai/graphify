use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::graph::CodeGraph;

/// Escapes a string for use inside XML attribute values and text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Writes the graph in GraphML (XML) format to `path`.
///
/// Produces a valid GraphML document with `<key>` declarations for all node and
/// edge attributes.  Compatible with yEd, Gephi, and other GraphML consumers.
///
/// # Panics
/// Panics if file I/O fails.
pub fn write_graphml(graph: &CodeGraph, path: &Path) {
    let mut buf = String::new();

    // XML header + GraphML root
    writeln!(buf, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(
        buf,
        r#"<graphml xmlns="http://graphml.graphstruct.org/xmlns""#
    )
    .unwrap();
    writeln!(
        buf,
        r#"         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#
    )
    .unwrap();
    writeln!(
        buf,
        r#"         xsi:schemaLocation="http://graphml.graphstruct.org/xmlns http://graphml.graphstruct.org/xmlns/1.0/graphml.xsd">"#
    )
    .unwrap();

    // Key declarations — node attributes
    writeln!(
        buf,
        r#"  <key id="kind" for="node" attr.name="kind" attr.type="string"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="file_path" for="node" attr.name="file_path" attr.type="string"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="language" for="node" attr.name="language" attr.type="string"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="line" for="node" attr.name="line" attr.type="int"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="is_local" for="node" attr.name="is_local" attr.type="boolean"/>"#
    )
    .unwrap();

    // Key declarations — edge attributes
    writeln!(
        buf,
        r#"  <key id="edge_kind" for="edge" attr.name="kind" attr.type="string"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="weight" for="edge" attr.name="weight" attr.type="int"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="edge_line" for="edge" attr.name="line" attr.type="int"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="confidence" for="edge" attr.name="confidence" attr.type="double"/>"#
    )
    .unwrap();
    writeln!(
        buf,
        r#"  <key id="confidence_kind" for="edge" attr.name="confidence_kind" attr.type="string"/>"#
    )
    .unwrap();

    // Graph element
    writeln!(buf, r#"  <graph id="G" edgedefault="directed">"#).unwrap();

    // Nodes
    for node in graph.nodes() {
        let id = xml_escape(&node.id);
        writeln!(buf, r#"    <node id="{id}">"#).unwrap();
        writeln!(buf, r#"      <data key="kind">{:?}</data>"#, node.kind).unwrap();
        writeln!(
            buf,
            r#"      <data key="file_path">{}</data>"#,
            xml_escape(&node.file_path.to_string_lossy())
        )
        .unwrap();
        writeln!(
            buf,
            r#"      <data key="language">{:?}</data>"#,
            node.language
        )
        .unwrap();
        writeln!(buf, r#"      <data key="line">{}</data>"#, node.line).unwrap();
        writeln!(
            buf,
            r#"      <data key="is_local">{}</data>"#,
            node.is_local
        )
        .unwrap();
        writeln!(buf, r#"    </node>"#).unwrap();
    }

    // Edges
    for (i, (src, tgt, edge)) in graph.edges().iter().enumerate() {
        writeln!(
            buf,
            r#"    <edge id="e{i}" source="{src}" target="{tgt}">"#,
            src = xml_escape(src),
            tgt = xml_escape(tgt),
        )
        .unwrap();
        writeln!(buf, r#"      <data key="edge_kind">{:?}</data>"#, edge.kind).unwrap();
        writeln!(buf, r#"      <data key="weight">{}</data>"#, edge.weight).unwrap();
        writeln!(buf, r#"      <data key="edge_line">{}</data>"#, edge.line).unwrap();
        writeln!(
            buf,
            r#"      <data key="confidence">{}</data>"#,
            edge.confidence
        )
        .unwrap();
        writeln!(
            buf,
            r#"      <data key="confidence_kind">{:?}</data>"#,
            edge.confidence_kind
        )
        .unwrap();
        writeln!(buf, r#"    </edge>"#).unwrap();
    }

    writeln!(buf, "  </graph>").unwrap();
    writeln!(buf, "</graphml>").unwrap();

    std::fs::write(path, buf).expect("write GraphML file");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{ConfidenceKind, Edge, Language, Node};

    fn make_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module(
            "app.main",
            "app/main.py",
            Language::Python,
            1,
            true,
        ));
        g.add_node(Node::module(
            "app.utils",
            "app/utils.py",
            Language::Python,
            1,
            true,
        ));
        g.add_edge("app.main", "app.utils", Edge::imports(3));
        g
    }

    #[test]
    fn write_graphml_creates_valid_xml_structure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.graphml");
        let graph = make_graph();

        write_graphml(&graph, &path);

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<?xml version="));
        assert!(content.contains("<graphml"));
        assert!(content.contains("edgedefault=\"directed\""));
        assert!(content.contains("</graphml>"));
    }

    #[test]
    fn write_graphml_contains_nodes_and_edges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.graphml");
        let graph = make_graph();

        write_graphml(&graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"<node id="app.main">"#));
        assert!(content.contains(r#"<node id="app.utils">"#));
        assert!(content.contains(r#"source="app.main""#));
        assert!(content.contains(r#"target="app.utils""#));
        assert!(content.contains("<data key=\"edge_kind\">Imports</data>"));
    }

    #[test]
    fn write_graphml_has_key_declarations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.graphml");
        let graph = make_graph();

        write_graphml(&graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"<key id="kind" for="node""#));
        assert!(content.contains(r#"<key id="file_path" for="node""#));
        assert!(content.contains(r#"<key id="edge_kind" for="edge""#));
        assert!(content.contains(r#"<key id="confidence" for="edge""#));
    }

    #[test]
    fn write_graphml_includes_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.graphml");

        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        g.add_edge(
            "a",
            "b",
            Edge::imports(1).with_confidence(0.85, ConfidenceKind::Inferred),
        );

        write_graphml(&g, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<data key=\"confidence\">0.85</data>"));
        assert!(content.contains("<data key=\"confidence_kind\">Inferred</data>"));
    }

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape(r#"a"b'c"#), "a&quot;b&apos;c");
        assert_eq!(xml_escape("normal"), "normal");
    }
}
