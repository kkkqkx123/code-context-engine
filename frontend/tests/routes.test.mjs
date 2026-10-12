import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const page = (rel) => join(root, 'src/routes', rel);
const read = (rel) => readFileSync(join(root, 'src/routes', rel), 'utf8');

const KEY_PAGES = [
	['dashboard', '+page.svelte', /PageHeader/],
	['projects', 'projects/+page.svelte', /project/i],
	['index', 'index/+page.svelte', /index/i],
	['search', 'search/+page.svelte', /searchActions|searchApi/],
	['entity detail', 'entities/[id]/+page.svelte', /entityActions/],
	['graph', 'graph/+page.svelte', /graphActions/],
	['tools', 'tools/+page.svelte', /toolsApi|Fold|Compress/],
	['config', 'config/+page.svelte', /configApi|config/i],
];

describe('key page main paths', () => {
	for (const [name, rel, marker] of KEY_PAGES) {
		it(`${name} page exists with its main wiring`, () => {
			assert.ok(existsSync(page(rel)), `${rel} must exist`);
			assert.match(read(rel), marker, `${rel} must reference ${marker}`);
		});
	}

	it('graph page covers all seed strategies', () => {
		const text = read('graph/+page.svelte');
		for (const marker of [
			'loadEgo',
			'loadPath',
			'loadSubgraph',
			'loadOverview',
			'loadComponentsIfStale',
			'loadImpact',
			'loadEntityImpact',
			'loadModule',
		]) {
			assert.ok(text.includes(marker), `graph page must cover ${marker}`);
		}
	});

	it('tools page exposes fold batch and compress entries', () => {
		const text = read('tools/+page.svelte');
		assert.ok(text.includes('Fold') || text.includes('fold'));
		assert.ok(text.includes('Compress') || text.includes('compress'));
	});
});
