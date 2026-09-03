fn main() {
    // Compile C parser
    cc::Build::new()
        .file("src/parser.c")
        .include("src")
        .compile("tree-sitter-vue-parser");

    // Compile C++ scanner
    cc::Build::new()
        .cpp(true)
        .file("src/scanner.cc")
        .include("src")
        .compile("tree-sitter-vue-scanner");

    // Tell cargo to rebuild if these files change
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/scanner.cc");
    println!("cargo:rerun-if-changed=grammar.js");
}
