//! Standard graph export formats (DOT, GraphML).
//!
//! Converts a `SubGraph` into Graphviz DOT or GraphML XML for external
//! visualization and analysis tools.

use super::graph::{GraphEdge, GraphNode, SubGraph};

/// Export a subgraph to Graphviz DOT format.
pub fn to_dot(graph: &SubGraph) -> String {
    let mut out = String::from("digraph relation_graph {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box, style=rounded];\n\n");

    for node in &graph.nodes {
        let label = escape_dot(&node.label);
        let kind = escape_dot(&node.kind);
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\", kind=\"{}\"];\n",
            node.id, label, kind
        ));
    }
    out.push('\n');

    for edge in &graph.edges {
        let relation = escape_dot(&edge.relation);
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            edge.source, edge.target, relation
        ));
    }

    out.push_str("}\n");
    out
}

/// Export a subgraph to GraphML XML format.
pub fn to_graphml(graph: &SubGraph) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    out.push_str("  <key id=\"label\" for=\"node\" attr.name=\"label\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n");
    out.push_str(
        "  <key id=\"relation\" for=\"edge\" attr.name=\"relation\" attr.type=\"string\"/>\n",
    );
    out.push_str("  <key id=\"domain\" for=\"edge\" attr.name=\"domain\" attr.type=\"string\"/>\n");
    out.push_str("  <graph id=\"relation_graph\" edgedefault=\"directed\">\n");

    for node in &graph.nodes {
        out.push_str(&format!("    <node id=\"{}\">\n", escape_xml(&node.id)));
        out.push_str(&format!(
            "      <data key=\"label\">{}</data>\n",
            escape_xml(&node.label)
        ));
        out.push_str(&format!(
            "      <data key=\"kind\">{}</data>\n",
            escape_xml(&node.kind)
        ));
        out.push_str("    </node>\n");
    }

    for (i, edge) in graph.edges.iter().enumerate() {
        out.push_str(&format!(
            "    <edge id=\"e{}\" source=\"{}\" target=\"{}\">\n",
            i,
            escape_xml(&edge.source),
            escape_xml(&edge.target)
        ));
        out.push_str(&format!(
            "      <data key=\"relation\">{}</data>\n",
            escape_xml(&edge.relation)
        ));
        out.push_str(&format!(
            "      <data key=\"domain\">{}</data>\n",
            escape_xml(&edge.domain)
        ));
        out.push_str("    </edge>\n");
    }

    out.push_str("  </graph>\n");
    out.push_str("</graphml>\n");
    out
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> SubGraph {
        SubGraph {
            nodes: vec![
                GraphNode {
                    id: "a".to_string(),
                    label: "func_a".to_string(),
                    kind: "function".to_string(),
                    source_file: "src/a.rs".to_string(),
                    source_location: "L1".to_string(),
                    scoped_name: None,
                    signature: None,
                },
                GraphNode {
                    id: "b".to_string(),
                    label: "func_b".to_string(),
                    kind: "function".to_string(),
                    source_file: "src/b.rs".to_string(),
                    source_location: "L10".to_string(),
                    scoped_name: None,
                    signature: None,
                },
            ],
            edges: vec![GraphEdge {
                source: "a".to_string(),
                target: "b".to_string(),
                relation: "call.direct".to_string(),
                domain: "call".to_string(),
                confidence: "extracted".to_string(),
                call_context: Some("direct".to_string()),
                is_external: false,
            }],
        }
    }

    #[test]
    fn dot_contains_nodes_and_edges() {
        let dot = to_dot(&sample_graph());
        assert!(dot.contains("digraph"));
        assert!(dot.contains("\"a\""));
        assert!(dot.contains("\"b\""));
        assert!(dot.contains("\"a\" -> \"b\""));
        assert!(dot.contains("call.direct"));
    }

    #[test]
    fn graphml_is_valid_xml_shape() {
        let graphml = to_graphml(&sample_graph());
        assert!(graphml.contains("<graphml"));
        assert!(graphml.contains("<graph"));
        assert!(graphml.contains("<node id=\"a\">"));
        assert!(graphml.contains("<edge"));
        assert!(graphml.contains("call.direct"));
    }

    #[test]
    fn escape_xml_handles_special_chars() {
        assert_eq!(escape_xml("a<b"), "a&lt;b");
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("\"q\""), "&quot;q&quot;");
    }

    #[test]
    fn escape_dot_handles_quotes() {
        assert_eq!(escape_dot("a\"b"), "a\\\"b");
    }
}
