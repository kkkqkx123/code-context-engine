import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const read = (rel) => readFileSync(join(root, rel), 'utf8');

describe('request-layer passthrough', () => {
	it('search store forwards intent and expand to both search modes', () => {
		const text = read('src/lib/stores/search.svelte.ts');
		assert.match(text, /query_intent/);
		assert.match(text, /expand/);
		assert.match(text, /buildCommonFilterFields/);
		assert.match(text, /aggregatedSearch/);
	});

	it('entity request layer maps the full RelationFilter', () => {
		const text = read('src/lib/api/entities.ts');
		for (const field of [
			'max_depth',
			'offset',
			'limit',
			'exclude_tests',
			'directory_prefix',
			'excluded_files',
			'domains',
			'include_external',
		]) {
			assert.ok(text.includes(field), `entities.ts must map ${field}`);
		}
		assert.ok(text.includes('getClassificationStats'));
		assert.ok(text.includes('getRelationsByClassification'));
		assert.ok(text.includes('/relations/classification/stats'));
		assert.ok(text.includes('/relations/classification/{classification}'));
	});

	it('graph request layer maps traversal filters on every endpoint', () => {
		const text = read('src/lib/api/graph.ts');
		for (const field of [
			'relation_types',
			'include_external',
			'domains',
			'offset',
			'limit',
		]) {
			assert.ok(text.includes(field), `graph.ts must map ${field}`);
		}
		assert.ok(text.includes('MAX_SUBGRAPH_IDS'));
	});

	it('single compress forwards entity and group toggles', () => {
		const api = read('src/lib/api/tools.ts');
		assert.ok(api.includes('/api/tools/compress'));
		const ui = read('src/lib/components/tools/CompressionTool.svelte');
		assert.ok(ui.includes('include_entities'));
		assert.ok(ui.includes('include_groups'));
	});

	it('batch fold forwards per-item tokens and mode plus globals', () => {
		const ui = read('src/lib/components/tools/FoldBatchTool.svelte');
		assert.ok(ui.includes('max_tokens: item.maxTokens'));
		assert.ok(ui.includes('mode: item.mode'));
		assert.ok(ui.includes('language: globalLanguage'));
		assert.ok(ui.includes('max_tokens: globalMaxTokens'));
		assert.ok(ui.includes('mode: globalMode'));
	});

	it('graph expand carries the server filter', () => {
		const page = read('src/routes/graph/+page.svelte');
		assert.ok(
			page.includes(
				'graphStore.expand(nodeId, 1, egoDirection, serverFilter())',
			),
			'node expansion must carry the server-side filter',
		);
	});
});
