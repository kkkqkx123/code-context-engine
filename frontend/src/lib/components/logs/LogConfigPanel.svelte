<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogConfig } from '$lib/api/logs';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';

	let config = $state<LogConfig | null>(null);
	let level = $state('info');
	let format = $state('pretty');
	let filter = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);

	async function load() {
		try {
			config = await logsApi.getConfig();
			level = config.level;
			format = config.format;
			filter = config.filter ?? '';
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	async function save() {
		saving = true;
		error = null;
		message = null;
		try {
			await logsApi.updateConfig({
				level,
				format,
				filter: filter || undefined,
			});
			message = 'Configuration updated';
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	onMount(load);
</script>

{#if config}
	<Card title="Current Configuration" subtitle="Read-only runtime values">
		<div class="spec-grid">
			<div class="spec-item">
				<span class="spec-label">Output</span>
				<span class="spec-value">{config.output}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">File</span>
				<span class="spec-value">{config.file ?? '-'}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">Max file size (MB)</span>
				<span class="spec-value">{config.max_file_size_mb ?? '-'}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">Max files</span>
				<span class="spec-value">{config.max_files ?? '-'}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">Max total size (MB)</span>
				<span class="spec-value">{config.max_total_size_mb ?? '-'}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">WS capacity</span>
				<span class="spec-value">{config.ws_capacity}</span>
			</div>
			<div class="spec-item">
				<span class="spec-label">Buffer size</span>
				<span class="spec-value">{config.memory_buffer_size}</span>
			</div>
		</div>
	</Card>
{/if}

<Card title="Update Logging" subtitle="Level, format and filter directives">
	{#if error}
		<InlineError message={error} />
	{/if}
	{#if message}
		<div class="success-note">
			<Badge label="Updated" variant="success" />
			<span>{message}</span>
		</div>
	{/if}
	<div class="form-grid">
		<label class="form-field" for="log-level">
			<span class="form-label">Level</span>
			<select id="log-level" class="text-input" bind:value={level}>
				<option value="trace">trace</option>
				<option value="debug">debug</option>
				<option value="info">info</option>
				<option value="warn">warn</option>
				<option value="error">error</option>
			</select>
		</label>
		<label class="form-field" for="log-format">
			<span class="form-label">Format</span>
			<select id="log-format" class="text-input" bind:value={format}>
				<option value="pretty">pretty</option>
				<option value="compact">compact</option>
				<option value="json">json</option>
			</select>
		</label>
	</div>
	<div class="full-row">
		<Input
			id="log-filter"
			label="Filter directives"
			bind:value={filter}
			placeholder="info,cce_server=debug"
		/>
	</div>
	<div class="form-actions">
		<Button onclick={save} disabled={saving}>
			{saving ? 'Saving...' : 'Save'}
		</Button>
	</div>
</Card>

<style>
	.spec-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
		gap: 1rem;
	}

	.spec-item {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		padding: 0.75rem;
		border: 1px solid var(--gray-200);
	}

	.spec-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.spec-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.82rem;
		word-break: break-all;
	}

	.success-note {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
		font-size: 0.85rem;
	}

	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.form-field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.form-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.full-row {
		margin-bottom: 1rem;
	}

	.form-actions {
		display: flex;
		justify-content: flex-end;
	}

	@media (max-width: 768px) {
		.form-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
