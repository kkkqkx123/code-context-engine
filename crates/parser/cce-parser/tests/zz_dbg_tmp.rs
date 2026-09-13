#[test]
fn zz_dbg_ts_accessor_ast() {
    let src = "class A { get foo(): string { return this.x; } set foo(v: string) {} bar(): number { return 1; } }";
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&lang).expect("lang");
    let tree = parser.parse(src, None).expect("parse");
    eprintln!("SEXPR: {}", tree.root_node().to_sexp());
}
