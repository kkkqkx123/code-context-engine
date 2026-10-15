// Source size baseline check: fails when any frontend source file grows past
// the line budget. Keeps routes/components/api modules from accumulating
// unreviewed bulk; intentionally cheap (line count) rather than AST-based.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const LINE_BUDGET = 1000;
const EXTENSIONS = new Set(['.ts', '.js', '.svelte', '.mjs', '.css', '.html']);
const SKIP_DIRS = new Set(['node_modules', '.svelte-kit', 'build', 'dist', 'scripts']);
// Auto-generated files (openapi-typescript output) are exempt from the line
// budget; their size tracks the server API surface, not hand-written code.
const SKIP_FILES = new Set(['schema.d.ts']);

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function* walk(dir) {
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		const stat = statSync(full);
		if (stat.isDirectory()) {
			if (!SKIP_DIRS.has(entry)) yield* walk(full);
		} else if (!SKIP_FILES.has(entry) && EXTENSIONS.has(entry.slice(entry.lastIndexOf('.')))) {
			yield full;
		}
	}
}

const offenders = [];
for (const file of walk(frontendRoot)) {
	const lines = readFileSync(file, 'utf8').split('\n').length;
	if (lines > LINE_BUDGET) {
		offenders.push({ file: relative(frontendRoot, file), lines });
	}
}

if (offenders.length > 0) {
	console.error(`Files exceeding the ${LINE_BUDGET}-line baseline:`);
	for (const { file, lines } of offenders) console.error(`  ${file}: ${lines} lines`);
	process.exit(1);
}

console.log(`baseline OK: all source files within ${LINE_BUDGET} lines`);
