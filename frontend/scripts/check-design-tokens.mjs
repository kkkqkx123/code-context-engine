#!/usr/bin/env node
/**
 * Keeps the shared design token catalogue in sync with the web stylesheet.
 *
 * `design/tokens.json` is the machine-readable source consumed by the desktop
 * client. The web studio keeps its variables in the `:root` block of
 * `src/app.css`, so this check fails when either side drifts: a variable
 * missing on either side and a value mismatch are all reported.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const frontend = fileURLToPath(new URL('..', import.meta.url));
const css = readFileSync(join(frontend, 'src/app.css'), 'utf8');
const tokens = JSON.parse(
	readFileSync(join(frontend, '..', 'design', 'tokens.json'), 'utf8'),
);

function declarations(text) {
	const result = {};
	const pattern = /(--[\w-]+)\s*:\s*([^;{}]+);/g;
	for (const match of text.matchAll(pattern)) {
		result[match[1]] = match[2].trim().replace(/\s+/g, ' ');
	}
	return result;
}

const rootMatch = css.match(/:root\s*\{([\s\S]*?)\}/);
if (!rootMatch) throw new Error('cannot locate the :root block in src/app.css');
const actual = declarations(rootMatch[1]);

const META_SECTIONS = new Set(['version', 'source', 'desktop']);
const expected = {};
for (const [section, entries] of Object.entries(tokens)) {
	if (META_SECTIONS.has(section)) continue;
	if (entries === null || typeof entries !== 'object' || Array.isArray(entries)) {
		console.error(`design/tokens.json: section "${section}" must be an object`);
		process.exit(1);
	}
	Object.assign(expected, entries);
}

const failures = [];
for (const [name, value] of Object.entries(expected)) {
	const normalized = String(value).trim().replace(/\s+/g, ' ');
	if (!(name in actual)) failures.push(`app.css :root is missing "${name}"`);
	else if (actual[name] !== normalized)
		failures.push(`"${name}" expected "${normalized}" but app.css has "${actual[name]}"`);
}
for (const name of Object.keys(actual)) {
	if (!(name in expected))
		failures.push(`app.css :root declares "${name}" which is absent from design/tokens.json`);
}

if (typeof tokens.desktop?.fontFamily !== 'string' || tokens.desktop.fontFamily.length === 0)
	failures.push('design/tokens.json is missing desktop.fontFamily');
if (typeof tokens.desktop?.accentActive !== 'string' || tokens.desktop.accentActive.length === 0)
	failures.push('design/tokens.json is missing desktop.accentActive');

if (failures.length > 0) {
	console.error(`design token check failed (${failures.length} problem(s)):`);
	for (const failure of failures) console.error(`  ${failure}`);
	process.exit(1);
}

console.log(
	`design token check passed: ${Object.keys(actual).length} variables in the :root block`,
);
