/**
 * Style consistency check: guards the frontend-unification invariants.
 * Fails when a page reintroduces a local pattern that now has a shared
 * building block (Tabs, Notice, Modal, SelectChip, ToolFormRow/ToolActions,
 * table-wrap/table, search-hero).
 *
 * Runs in CI via `npm run check:style`. Scans only `src/`, never build output.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const SRC = new URL('../src/', import.meta.url).pathname;

function walk(dir) {
	const out = [];
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		if (statSync(full).isDirectory()) out.push(...walk(full));
		else if (full.endsWith('.svelte')) out.push(full);
	}
	return out;
}

const files = walk(SRC);

/** { pattern, message, allow } — allow lists files that legitimately define the pattern. */
const rules = [
	{
		pattern: /class="[^"]*\bwarn-banner\b/,
		message:
			'warn-banner is retired; use the Notice component (variant="warning").',
		allow: [],
	},
	{
		pattern: /class="[^"]*\binfo-banner\b/,
		message:
			'info-banner is retired; use the Notice component (variant="info").',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bstale-banner\b/,
		message:
			'stale-banner is retired; use the Notice component (variant="warning").',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bdead-table\b/,
		message: 'dead-table is retired; use .table-wrap + .table.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\btab-navigation\b/,
		message: 'tab-navigation is retired; use the Tabs component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bresult-tabs\b/,
		message: 'result-tabs is retired; use the Tabs component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bdomain-toggle\b/,
		message: 'domain-toggle is retired; use the SelectChip component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bcandidate-btn\b/,
		message: 'candidate-btn is retired; use the SelectChip component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bchip-btn\b/,
		message: 'chip-btn is retired; use the SelectChip component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bmodal-backdrop\b/,
		message: 'hand-written modal-backdrop is retired; use the Modal component.',
		allow: ['lib/components/ui/Modal.svelte'],
	},
	{
		pattern: /--color-(surface|border|text-muted|link|warning)\b/,
		message: 'ops dark --color-* tokens are retired; use --ops-* tokens.',
		allow: [],
	},
];

const offenders = [];

for (const file of files) {
	const rel = relative(SRC, file);
	const content = readFileSync(file, 'utf8');
	// Only inspect scoped styles + markup outside the components that own the pattern.
	for (const rule of rules) {
		if (rule.allow.some((a) => rel.endsWith(a))) continue;
		if (rule.pattern.test(content)) {
			offenders.push(`${rel}: ${rule.message}`);
		}
	}
	// Scoped redefinition of the global hero input is never allowed.
	if (rel !== 'app.css' && /\.search-input\s*\{/.test(content)) {
		offenders.push(
			`${rel}: scoped .search-input redefinition; use .text-input or the global .search-hero.`,
		);
	}
	// Tool layout must compose ToolFormRow/ToolActions, not hand-written divs.
	if (
		rel.startsWith('lib/components/tools/') &&
		!rel.endsWith('ToolShell.svelte') &&
		!rel.endsWith('ToolFormRow.svelte') &&
		!rel.endsWith('ToolActions.svelte') &&
		!rel.endsWith('ToolField.svelte')
	) {
		if (/class="tool-form-row"/.test(content)) {
			offenders.push(
				`${rel}: hand-written .tool-form-row; compose the ToolFormRow component.`,
			);
		}
		if (/class="tool-actions"/.test(content)) {
			offenders.push(
				`${rel}: hand-written .tool-actions; compose the ToolActions component.`,
			);
		}
	}
}

if (offenders.length > 0) {
	console.error('Style consistency check failed:');
	for (const line of offenders) console.error(`  ${line}`);
	process.exit(1);
}

console.log(`Style consistency check passed (${files.length} components).`);
