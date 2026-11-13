/**
 * Style consistency check: guards the frontend-unification invariants.
 * Fails when a page reintroduces a local pattern that now has a shared
 * building block (Tabs, Notice, Modal, SelectChip, ToolFormRow/ToolActions,
 * table-wrap/table, search-hero, Pagination, StatusDot, KpiCard,
 * DescriptionList, FieldCheck, Skeleton) or bypasses the design tokens
 * (--font-sans, --font-mono, tracking tokens, canonical sizes, spacing
 * scale, semantic borders).
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
		pattern: /class="[^"]*\bfile-link\b/,
		message: 'file-link is retired; use the global .link-btn class.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bproject-item\b/,
		message:
			'project-item is retired; use the global .selectable-row class (rows) or .selectable-row.selectable-col (cards).',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bsearch-focus-btn\b/,
		message:
			'search-focus-btn is retired; use the Button component (variant="ghost", size="xs").',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bkind-select\b/,
		message: 'kind-select is retired; use the global .select class.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bsearch-bar\b/,
		message: 'search-bar is retired; use the global .tool-form-row class.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bfilter-check\b/,
		message: 'filter-check is retired; use the FieldCheck component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bhome-search-input\b/,
		message: 'home-search-input is retired; use the global .search-hero class.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bhdot\b/,
		message: 'hdot is retired; use the StatusDot component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bstatus-dot\b/,
		message: 'hand-written status-dot is retired; use the StatusDot component.',
		allow: ['lib/components/ui/StatusDot.svelte'],
	},
	{
		pattern: /class="[^"]*\bdetail-grid\b/,
		message:
			'detail-grid is retired; use DescriptionList (term/value data) or .card-grid (card collections).',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bdetail-(item|value)\b/,
		message:
			'detail-item/detail-value are retired; use the DescriptionList component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\brelationship-(item|list)\b/,
		message:
			'relationship-item/list are retired; use .scroll-list with .selectable-row buttons.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bselect-all\b/,
		message: 'select-all is retired; use the FieldCheck component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bpage-label\b/,
		message: 'page-label is retired; use the Pagination component.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bkv-(list|row|label|value)\b/,
		message:
			'kv-list/row/label/value are retired; use .desc-list/.desc-row/.desc-term/.desc-value.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bservice-grid\b/,
		message: 'service-grid is retired; use the global .card-grid class.',
		allow: [],
	},
	{
		pattern: /class="[^"]*\bcommunity-btn\b/,
		message: 'community-btn is retired; use the SelectChip component.',
		allow: [],
	},
	{
		pattern: /#[0-9a-fA-F]{3}\b|#[0-9a-fA-F]{6}\b/,
		message: 'hard-coded hex color is retired; use a var(--*) token.',
		allow: [],
	},
	{
		pattern: /font-family:\s*['"]Space/,
		message:
			'literal Space font stack is retired; use var(--font-sans) or var(--font-mono).',
		allow: [],
	},
	{
		pattern: /font-family:\s*monospace/,
		message:
			'bare monospace is retired; use var(--font-mono) for the full stack.',
		allow: [],
	},
	{
		pattern: /font-weight:\s*(bold|600)\b/,
		message:
			'bold keyword and 600 are retired; use 400 or 700 matching the loaded faces.',
		allow: [],
	},
	{
		pattern: /font-size:\s*[\d.]+em;/,
		message: 'em font-size is retired; use rem for predictable scaling.',
		allow: [],
	},
	{
		pattern: /letter-spacing:\s*(0\.15em|0\.16em|-0\.04em|-0\.02em)/,
		message: 'outlier tracking is retired; use the canonical tracking tokens.',
		allow: [],
	},
	{
		pattern:
			/font-size:\s*(0\.62rem|0\.68rem|0\.72rem|0\.78rem|0\.82rem|0\.95rem|1\.05rem)/,
		message:
			'non-canonical size is retired; use the nearest canonical text scale step.',
		allow: [],
	},
	{
		pattern: /gap:\s*(0\.15rem|0\.3rem|0\.35rem|0\.4rem|0\.6rem)/,
		message:
			'outlier gap is retired; use the nearest spacing-scale step (0.125rem, 0.25rem, 0.5rem, 0.75rem, 1rem, 1.5rem, 2rem).',
		allow: [],
	},
	{
		pattern: /(padding|gap):[^;{}]*[0-9]px/,
		message: 'px spacing is retired; use rem spacing-scale steps.',
		allow: [],
	},
];

/** Scoped redefinitions of promoted globals. Only the owning component
 *  (listed in `owner`) may define the pattern; every other file reuses it. */
const scopedRules = [
	{
		pattern: /\.selectable-row\s*\{/,
		message:
			'scoped .selectable-row redefinition; use the global class (add .selectable-col for vertical cards).',
	},
	{
		pattern: /\.more-toggle\s*\{/,
		message: 'scoped .more-toggle redefinition; use the global class.',
	},
	{
		pattern: /\.link-btn\s*\{/,
		message: 'scoped .link-btn redefinition; use the global class.',
	},
	{
		pattern: /\.suggest-chip\s*\{/,
		message: 'scoped .suggest-chip redefinition; use the global class.',
	},
	{
		pattern: /\.section-heading\s*\{/,
		message:
			'scoped .section-heading redefinition; use the global class (card headings use .section-title, panel headings use .eyebrow-title).',
	},
	{
		pattern: /\.desc-(list|item|term|value|value-mono|row)\b\s*\{/,
		message:
			'scoped .desc-* redefinition; use the global classes or the DescriptionList component.',
	},
	{
		pattern: /\.card-grid\s*\{/,
		message: 'scoped .card-grid redefinition; use the global class.',
	},
	{
		pattern: /\.check-row\s*\{/,
		message: 'scoped .check-row redefinition; use the FieldCheck component.',
	},
	{
		pattern: /\.status-dot\s*\{/,
		message: 'scoped .status-dot redefinition; use the StatusDot component.',
		owner: 'lib/components/ui/StatusDot.svelte',
	},
	{
		pattern: /\.skeleton(-line)?\s*\{/,
		message: 'scoped .skeleton redefinition; use the Skeleton component.',
	},
	{
		pattern: /\.select\s*\{/,
		message: 'scoped .select redefinition; use the global class.',
	},
	{
		pattern: /\.truncate\b\s*\{/,
		message: 'scoped .truncate redefinition; use the global class.',
	},
	{
		pattern: /\.scroll-list\s*\{/,
		message: 'scoped .scroll-list redefinition; use the global class.',
	},
	{
		pattern: /\.pagination\s*\{/,
		message: 'scoped .pagination redefinition; use the Pagination component.',
	},
	{
		pattern: /\.page-info\s*\{/,
		message: 'scoped .page-info redefinition; use the Pagination component.',
	},
	{
		pattern: /\bkbd\s*\{/,
		message: 'scoped kbd redefinition; use the global kbd style.',
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
	// Scoped redefinition of promoted globals is never allowed outside the owner.
	for (const rule of scopedRules) {
		if (rule.owner && rel.endsWith(rule.owner)) continue;
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
