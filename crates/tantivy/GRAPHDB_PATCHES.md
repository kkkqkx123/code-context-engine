# GraphDB Tantivy Branch Supplementary Notes

This branch adds only one capability for GraphDB: **configurable BM25 parameters**.

## New Content

### `Bm25Params`

A new `Bm25Params` struct is added to configure the BM25 parameters for each index:

- `k1`: Term frequency saturation parameter
- `b`: Document length normalization parameter

Default values remain unchanged:

- `k1 = 1.2`
- `b = 0.75`

### `IndexSettings::bm25_params`

An optional field is added to `IndexSettings`:

- `bm25_params: Option<Bm25Params>`

If not explicitly set, behavior remains identical to upstream, still using the default BM25 parameters.

### Serialization Compatibility

`bm25_params` will be written to `meta.json`, but old index files missing this field can still be read normally.

## Usage Example

```rust
use tantivy::index::Bm25Params;
use tantivy::{Index, IndexSettings};

let settings = IndexSettings {
    bm25_params: Some(Bm25Params { k1: 1.5, b: 0.6 }),
    ..Default::default()
};

let index = Index::create_in_dir(path, schema, settings)?;
let params = index.bm25_params();
assert_eq!(params.k1, 1.5);
assert_eq!(params.b, 0.6);
```

## Notes

- This branch documentation describes only the features added by this branch and does not repeat content already present in upstream.
- If `bm25_params` is not used, this branch maintains search behavior consistent with upstream.