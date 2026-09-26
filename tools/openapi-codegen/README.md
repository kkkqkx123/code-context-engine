# openapi-codegen

On-demand TypeScript codegen from the committed cce-server OpenAPI snapshot.
This package is intentionally isolated from the frontend dependency tree:
`openapi-typescript` requires TypeScript 5, while the frontend uses TypeScript 6.

## Usage

```sh
# regenerate from the committed snapshot
npm run gen
# copy the result into the frontend source tree (checked in)
cp schema.d.ts ../../frontend/src/lib/api/schema.d.ts
```

`node_modules` is not checked in. The generated `schema.d.ts` in the
frontend tree is the checked-in contract artifact.
