/**
 * Build isolation check: the default (non-preview) build must not contain any
 * preview/mock code. Runs automatically after `npm run build`.
 *
 * Fails when markers that only exist in the preview branch (mock payloads,
 * mock client, preview env flag) leak into the build output.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const BUILD_DIR = new URL('../build/', import.meta.url).pathname;

/** Distinctive strings that only appear in preview-only modules. */
const MARKERS = [
	'preview/mock/',
	'mockClient',
	'mockDelay',
	'mockProjects',
	'VITE_USE_MOCK',
];

function walk(dir) {
	const out = [];
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		if (statSync(full).isDirectory()) out.push(...walk(full));
		else out.push(full);
	}
	return out;
}

const files = walk(BUILD_DIR);
const offenders = [];

for (const file of files) {
	const content = readFileSync(file, 'utf8');
	for (const marker of MARKERS) {
		if (content.includes(marker)) {
			offenders.push(`${file} contains "${marker}"`);
		}
	}
}

if (offenders.length > 0) {
	console.error('Default build contains preview/mock code:');
	for (const line of offenders) console.error(`  ${line}`);
	console.error(
		'Check that no shared module imports src/lib/preview or client.preview.ts.',
	);
	process.exit(1);
}

console.log(
	`Build isolation check passed (${files.length} files, no preview/mock markers).`,
);
