<script lang="ts">
	/**
	 * Global validation summary card for the config page: cross-module
	 * check results for the active configuration. The page owns the
	 * validation request.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import type { ConfigValidateResponse } from '$lib/api/config';

	interface Props {
		validateResult: ConfigValidateResponse | null;
		validating?: boolean;
		onValidate: () => void;
	}

	let { validateResult, validating = false, onValidate }: Props = $props();
</script>

<Card
	title="Global Validation"
	subtitle="Cross-module checks for the active config"
>
	{#if validateResult}
		<div class="validate-status">
			<span class="validate-label">Status</span>
			<Badge
				label={validateResult.valid ? 'Valid' : 'Invalid'}
				variant={validateResult.valid ? 'active' : 'inactive'}
			/>
			<span style="flex: 1"></span>
			<Button onclick={onValidate} disabled={validating}>
				{#if validating}Checking...{:else}Re-validate{/if}
			</Button>
		</div>

		{#if validateResult.errors.length > 0}
			<div class="issue-section">
				<h3 class="section-title">
					Errors ({validateResult.errors.length})
				</h3>
				<ul class="issue-list">
					{#each validateResult.errors as err (err)}
						<li class="issue-item error">{err}</li>
					{/each}
				</ul>
			</div>
		{/if}

		{#if validateResult.warnings.length > 0}
			<div class="issue-section">
				<h3 class="section-title">
					Warnings ({validateResult.warnings.length})
				</h3>
				<ul class="issue-list">
					{#each validateResult.warnings as warn (warn)}
						<li class="issue-item warning">{warn}</li>
					{/each}
				</ul>
			</div>
		{/if}

		{#if validateResult.dependency_warnings.length > 0}
			<div class="issue-section">
				<h3 class="section-title">
					Dependency Warnings ({validateResult.dependency_warnings.length})
				</h3>
				<ul class="issue-list">
					{#each validateResult.dependency_warnings as dw (dw.field)}
						<li class="issue-item warning">
							<strong>{dw.field}:</strong>
							{dw.suggestion}
						</li>
					{/each}
				</ul>
			</div>
		{/if}

		{#if validateResult.valid && validateResult.errors.length === 0 && validateResult.warnings.length === 0 && validateResult.dependency_warnings.length === 0}
			<p class="empty-hint">No issues found — configuration is clean.</p>
		{/if}
	{:else}
		<p class="empty-hint">Loading validation results...</p>
	{/if}
</Card>

<style>
	.validate-status {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}




	.issue-section {
		margin-top: 1.5rem;
	}

	.issue-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.issue-item {
		padding: 0.75rem 1rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		border: var(--border-muted);
		line-height: 1.5;
	}

	.issue-item.error {
		border-color: var(--danger);
		color: var(--danger);
		background: var(--danger-bg);
	}

	.issue-item.warning {
		border-color: var(--warning);
		color: var(--warning);
		background: var(--warning-bg);
	}

</style>
