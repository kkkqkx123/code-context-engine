// Verifies that the generated OpenAPI wire contract (src/lib/api/schema.d.ts)
// stays in sync with the server's OpenAPI specification
// (tools/openapi-codegen/openapi.json). Every path exposed by the server must
// exist in the generated client schema.
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const specPath = resolve(root, 'tools/openapi-codegen/openapi.json');
const schemaPath = resolve(root, 'frontend/src/lib/api/schema.d.ts');

const spec = JSON.parse(readFileSync(specPath, 'utf8'));
const schema = readFileSync(schemaPath, 'utf8');

const missing = Object.keys(spec.paths ?? {}).filter(
	(path) => !schema.includes(`"${path}"`),
);

if (missing.length > 0) {
	console.error(`schema.d.ts is out of date with the server OpenAPI spec.`);
	console.error(`Missing paths (${missing.length}):`);
	for (const path of missing) console.error(`  - ${path}`);
	console.error(
		`Regenerate with: openapi-typescript tools/openapi-codegen/openapi.json -o frontend/src/lib/api/schema.d.ts`,
	);
	process.exit(1);
}

console.log(
	`contract OK: ${Object.keys(spec.paths ?? {}).length} paths match schema.d.ts`,
);
