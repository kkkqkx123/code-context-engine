<script lang="ts">
	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		label?: string;
		type?: 'text' | 'number' | 'email' | 'password' | 'search' | 'select';
		placeholder?: string;
		value?: string;
		error?: string;
		required?: boolean;
		id?: string;
		multiline?: boolean;
		options?: Option[];
		[key: string]: unknown;
	}

	let {
		label = '',
		type = 'text',
		placeholder = '',
		value = $bindable(''),
		error = '',
		required = false,
		id = '',
		multiline = false,
		options = [],
		...rest
	}: Props = $props();

	let inputEl: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement | undefined =
		$state();

	// Generate unique ID if not provided
	let inputId = $derived(
		id || `input-${Math.random().toString(36).slice(2, 11)}`,
	);

	export function focus() {
		inputEl?.focus();
	}
</script>

<div class="input-wrapper">
	{#if label}
		<label class="label" for={inputId}>
			{label}
			{#if required}
				<span class="required">*</span>
			{/if}
		</label>
	{/if}

	{#if type === 'select'}
		<select bind:this={inputEl} id={inputId} class="select" {placeholder} bind:value {...rest}>
			{#each options as opt (opt.value)}
				<option value={opt.value}>{opt.label}</option>
			{/each}
		</select>
	{:else if multiline}
		<textarea
			bind:this={inputEl}
			id={inputId}
			class="textarea"
			{placeholder}
			bind:value
			{...rest}></textarea>
	{:else}
		<input
			bind:this={inputEl}
			id={inputId}
			class="input"
			{type}
			{placeholder}
			bind:value
			{...rest}
		/>
	{/if}

	{#if error}
		<p class="error-message">{error}</p>
	{/if}
</div>

<style>
	.input-wrapper {
		width: 100%;
		margin-bottom: 1.5rem;
	}

	.required {
		color: var(--danger);
	}

	.error-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--danger);
		margin-top: 0.5rem;
	}
</style>
