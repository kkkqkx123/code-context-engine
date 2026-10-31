import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const read = (rel) => readFileSync(join(root, rel), 'utf8');

describe('state-layer passthrough', () => {
	it('entity actions accept a RelationFilter for calls and chains', () => {
		const text = read('src/lib/stores/entities.svelte.ts');
		assert.match(text, /loadFunction\(id: string, filter\?: RelationFilter\)/);
		assert.match(text, /loadCallChain\(\s*id: string,/);
		assert.ok(text.includes('entityApi.getCalls(projId, id, filter)'));
		assert.ok(text.includes('entityApi.getCallers(projId, id, filter)'));
	});

	it('entity store exposes classification stats and detail', () => {
		const text = read('src/lib/stores/entities.svelte.ts');
		assert.ok(text.includes('loadClassificationStats'));
		assert.ok(text.includes('loadRelationsByClassification'));
		assert.ok(text.includes('classificationStats'));
		assert.ok(text.includes('classificationRelations'));
		assert.ok(text.includes('entityApi.getRelationsByClassification('));
	});

	it('graph actions accept a server-side filter', () => {
		const text = read('src/lib/stores/graph.svelte.ts');
		assert.ok(text.includes('filter?: GraphFilter'));
		assert.ok(text.includes('...filter'));
	});

	it('entity detail page wires filter controls to the store', () => {
		const text = read('src/routes/entities/[id]/+page.svelte');
		for (const marker of [
			'filterMaxDepth',
			'filterLimit',
			'filterExcludeTests',
			'filterDirectoryPrefix',
			'filterDomains',
			'filterIncludeExternal',
			'relationFilter()',
		]) {
			assert.ok(text.includes(marker), `detail page must wire ${marker}`);
		}
	});

	it('entity list page pairs classification panel with request funcs', () => {
		const text = read('src/routes/entities/+page.svelte');
		assert.ok(text.includes('runClassificationStats'));
		assert.ok(text.includes('runClassificationDetail'));
		assert.ok(text.includes('loadClassificationStats'));
		assert.ok(text.includes('loadRelationsByClassification'));
	});
});
