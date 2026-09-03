<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label?: string;
		type?: string;
		placeholder?: string;
		value?: string;
		error?: string;
		required?: boolean;
		id?: string;
		multiline?: boolean;
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
		...rest
	}: Props = $props();

	let inputEl: HTMLInputElement | HTMLTextAreaElement | undefined = $state();

	// Generate unique ID if not provided
	let inputId = $derived(id || `input-${Math.random().toString(36).slice(2, 11)}`);

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

	{#if multiline}
		<textarea
			bind:this={inputEl}
			id={inputId}
			class="textarea"
			{placeholder}
			bind:value
			{...rest}
		></textarea>
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

	.label {
		display: block;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.required {
		color: var(--danger);
	}

	.input,
	.textarea {
		width: 100%;
		padding: 0.75rem 1rem;
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		color: var(--black);
		background: var(--white);
		border: 1px solid var(--black);
		outline: none;
		transition: border-color 0.3s;
	}

	.input:focus,
	.textarea:focus {
		border-color: var(--accent);
	}

	.textarea {
		min-height: 150px;
		resize: vertical;
	}

	.error-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--danger);
		margin-top: 0.5rem;
	}
</style>
