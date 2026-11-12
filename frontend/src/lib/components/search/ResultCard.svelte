<script lang="ts">
	import type { SearchResultItem } from '$lib/api/search';
	import CodeBlock from '../ui/CodeBlock.svelte';
	import Badge from '../ui/Badge.svelte';
	import Button from '../ui/Button.svelte';
	import { languageFromPath } from '$lib/utils/code';

	interface Props {
		result: SearchResultItem;
		onNavigate?: (id: string) => void;
		/** Keyboard selection (j/k) highlight. */
		selected?: boolean;
		/** Query terms to mark inside the code chunk. */
		queryTerms?: string[];
	}

	let {
		result,
		onNavigate = () => {},
		selected = false,
		queryTerms = [],
	}: Props = $props();

	function formatScore(score: number | undefined): string {
		if (typeof score !== 'number' || Number.isNaN(score)) return 'n/a';
		return (score * 100).toFixed(1);
	}

	const hasScore = $derived(typeof result.score === 'number');
	const entityId = $derived(result.entity_ids?.[0] ?? null);
	const entityNames = $derived(
		result.entity_names?.slice(0, 3).join(', ') ?? '',
	);
	const extraEntities = $derived(
		(result.entity_ids?.length ?? 0) > 1
			? `+${(result.entity_ids?.length ?? 0) - 1} entities`
			: null,
	);
	const expansion = $derived(
		result.expanded ? `+${result.expanded_nodes ?? 0} related` : null,
	);
	let copied = $state(false);
	let pathCopied = $state(false);

	function handleNavigate() {
		if (entityId !== null) onNavigate(String(entityId));
	}

	async function copyCode() {
		if (!result.code_chunk) return;
		try {
			await navigator.clipboard.writeText(result.code_chunk);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			// Clipboard may be unavailable (e.g. non-secure origin); the code
			// stays selectable in the block above so it can be copied manually.
		}
	}

	/** Copy an IDE-friendly location instead of shipping a source viewer. */
	async function copyPath() {
		const location = `${result.file_path}:${result.start_line ?? 1}`;
		try {
			await navigator.clipboard.writeText(location);
			pathCopied = true;
			setTimeout(() => (pathCopied = false), 1500);
		} catch {
			// Clipboard may be unavailable; the path is rendered as text above
			// so it can be selected and copied manually.
		}
	}
</script>

<!-- The card is no longer a giant button: code must stay selectable/copyable. -->
<div class="result-card" class:selected role="group">
	<div class="result-header">
		<div class="result-meta">
			<span class="file-path">{result.file_path}</span>
			<span class="line-numbers"
				>:{result.start_line}{result.end_line
					? `-${result.end_line}`
					: ''}</span
			>
		</div>
		<div class="result-badges">
			{#if hasScore}
				<Badge variant={result.score > 0.8 ? 'active' : 'default'}>
					Score: {formatScore(result.score)}%
				</Badge>
			{/if}
			{#if result.entity_type}
				<Badge variant="default">{result.entity_type}</Badge>
			{/if}
			{#if result.sources && result.sources.length > 0}
				<Badge variant="default">{result.sources.join('+')}</Badge>
			{/if}
			{#if entityNames}
				<Badge variant="default">{entityNames}</Badge>
			{/if}
			{#if expansion}
				<Badge variant="default">{expansion}</Badge>
			{/if}
			{#if extraEntities}
				<Badge variant="default">{extraEntities}</Badge>
			{/if}
			{#if result.content_state && result.content_state !== 'full'}
				<Badge variant="default">{result.content_state}</Badge>
			{/if}
		</div>
	</div>

	{#if result.code_chunk}
		<div class="code-preview">
			<CodeBlock
				code={result.code_chunk}
				language={languageFromPath(result.file_path)}
				showLineNumbers={true}
				startLine={result.start_line ?? 1}
				highlightTerms={queryTerms}
			/>
			<div class="code-actions">
				<Button variant="secondary" size="xs" onclick={copyPath}>
					{pathCopied ? 'Copied' : 'Copy path:line'}
				</Button>
				<Button variant="ghost" size="xs" onclick={copyCode}>
					{copied ? 'Copied' : 'Copy code'}
				</Button>
				{#if entityId !== null}
					<Button variant="secondary" size="xs" onclick={handleNavigate}>
						Open entity
					</Button>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	/* Clickable primary result unit: black border (per border convention) */
	.result-card {
		border: 1px solid var(--black);
		padding: 1.5rem;
	}

	.result-card.selected {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.result-header {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		margin-bottom: 1rem;
		align-items: start;
	}

	.result-meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.file-path {
		word-break: break-all;
	}

	.line-numbers {
		color: var(--gray-400);
	}

	.result-badges {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.code-preview {
		margin-top: 1rem;
	}

	.code-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
</style>
