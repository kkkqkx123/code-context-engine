<script lang="ts">
	/**
	 * Remote access token card for the config page: browser-local admission
	 * credential for remote hosts. Owns the token draft; save and clear
	 * actions are reported to the page, which performs the store update and
	 * the server reachability check.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import { apiToken } from '$lib/stores/auth';

	interface Props {
		tokenDraft?: string;
		tokenSaving?: boolean;
		tokenMessage?: string | null;
		onSave: () => void;
		onClear: () => void;
	}

	let { tokenDraft = $bindable(''), tokenSaving = false, tokenMessage = null, onSave, onClear }: Props = $props();
</script>

<Card
	title="Remote Access Token"
	subtitle="Browser-local admission credential for remote hosts"
>
	<p class="reload-description">
		Stored only in this browser. Leave empty for local loopback use;
		paste a project-scoped token when the backend requires admission.
	</p>
	<div class="form-grid">
		<label class="form-field">
			<span class="form-label">Admission token</span>
			<Input
				type="password"
				bind:value={tokenDraft}
				placeholder="Paste token for this remote host"
				autocomplete="off"
			/>
		</label>
	</div>
	<div class="reload-actions">
		<Button onclick={onSave} disabled={tokenSaving}>
			{#if tokenSaving}Saving...{:else}Save Token{/if}
		</Button>
		<Button
			variant="secondary"
			onclick={onClear}
			disabled={tokenSaving || (!tokenDraft && !$apiToken)}
		>
			Clear
		</Button>
		{#if $apiToken}
			<Badge label="Token set" variant="active" />
		{:else}
			<Badge label="Local mode" variant="inactive" />
		{/if}
	</div>
	{#if tokenMessage}
		<p class="reload-description">{tokenMessage}</p>
	{/if}
</Card>

<style>
	.reload-description {
		color: var(--gray-600);
		margin-bottom: 1.5rem;
		line-height: 1.6;
	}

	.reload-actions {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 1.5rem;
	}

	.form-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.form-field {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.form-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}
</style>
