// Quick test to dump Bash and Lua node types
#[cfg(test)]
mod node_type_dump_tests {
    #[test]
    fn dump_bash_node_types() {
        // Use tree-sitter to parse a sample and get node types
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_bash::LANGUAGE.into()).unwrap();
        
        let tree = parser.parse("a=1", None).unwrap();
        let root = tree.root_node();
        
        // Print the tree structure
        println!("Bash parse tree for 'a=1':");
        print_node(root, 0);
        
        // Try parsing a file redirect
        let tree2 = parser.parse("cat < file", None).unwrap();
        println!("\nBash parse tree for 'cat < file':");
        print_node(tree2.root_node(), 0);
        
        // Try parsing heredoc  
        let tree3 = parser.parse("cat << EOF\nhello\nEOF", None).unwrap();
        println!("\nBash parse tree for heredoc:");
        print_node(tree3.root_node(), 0);
    }

    #[test]
    fn dump_lua_node_types() {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_lua::LANGUAGE.into()).unwrap();
        
        let tree = parser.parse("local x", None).unwrap();
        println!("Lua parse tree for 'local x':");
        print_node(tree.root_node(), 0);
        
        let tree2 = parser.parse("local x = 42", None).unwrap();
        println!("\nLua parse tree for 'local x = 42':");
        print_node(tree2.root_node(), 0);
    }

    fn print_node(node: tree_sitter::Node, depth: usize) {
        let indent = "  ".repeat(depth);
        let field_name = node.field_name().map(|f| format!("{}: ", f)).unwrap_or_default();
        println!("{}{}{} [{}, {}]", indent, field_name, node.kind(), node.start_position(), node.end_position());
        
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node(child, depth + 1);
        }
    }
}