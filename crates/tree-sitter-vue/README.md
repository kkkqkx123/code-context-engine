# tree-sitter-vue

Vue ([Vue v2.6.0 Template Syntax](https://vuejs.org/v2/guide/syntax.html)) grammar for [tree-sitter](https://github.com/tree-sitter/tree-sitter)

Compatible with **tree-sitter 0.20 ~ 0.26+**

_Note: This grammar is not responsible for parsing embedded languages, see [Multi-language Documents](http://tree-sitter.github.io/tree-sitter/using-parsers#multi-language-documents) for more info._

[Changelog](https://github.com/kkkqkx/tree-sitter-vue/blob/master/CHANGELOG.md)

## Install

### NPM

```sh
npm install tree-sitter-vue tree-sitter
```

### Cargo (Rust)

```sh
cargo add tree-sitter-vue
```

Or add to Cargo.toml:

```toml
[dependencies]
tree-sitter = "0.26"
tree-sitter-vue = "VERSION"
```

## Usage

### JavaScript

```js
const Parser = require("tree-sitter");
const Vue = require("tree-sitter-vue");

const parser = new Parser();
parser.setLanguage(Vue);

const sourceCode = `
<template>
  Hello, <a :[key]="url">{{ name }}</a>!
</template>
`;

const tree = parser.parse(sourceCode);
console.log(tree.rootNode.toString());
```

### Rust

```rust
use tree_sitter::Parser;
use tree_sitter_vue::language;

let mut parser = Parser::new();
parser.set_language(language()).expect("Failed to load Vue grammar");

let source_code = r#"
<template>
  Hello, <a :[key]="url">{{ name }}</a>!
</template>
"#;

let tree = parser.parse(source_code, None).unwrap();
println!("{}", tree.root_node().to_sexp());
```

## License

MIT © [Ika](https://github.com/kkkqkx123)
