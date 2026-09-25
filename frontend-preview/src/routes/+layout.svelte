<script lang="ts">
	import '../app.css';
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/state';
	import ToastContainer from '$lib/components/ui/ToastContainer.svelte';
	import { isOnline } from '$lib/stores/network';
	import { healthState, healthActions } from '$lib/stores/health';
	import { metricsState, metricsActions } from '$lib/stores/metrics';
	import { projects, loadProjects } from '$lib/stores/index';
	import { currentProjectId } from '$lib/stores/project';

	interface NavItem {
		href: string;
		label: string;
	}
	interface NavGroup {
		title: string;
		items: NavItem[];
	}

	const navGroups: NavGroup[] = [
		{ title: 'Overview', items: [{ href: '/', label: 'Dashboard' }] },
		{
			title: 'Data',
			items: [
				{ href: '/projects', label: 'Projects' },
				{ href: '/index', label: 'Index' },
				{ href: '/watch', label: 'Watch' },
			],
		},
		{
			title: 'Search',
			items: [
				{ href: '/search', label: 'Search' },
				{ href: '/entities', label: 'Entities' },
				{ href: '/graph', label: 'Graph' },
				{ href: '/summary', label: 'Summary' },
			],
		},
		{
			title: 'System',
			items: [
				{ href: '/storage', label: 'Storage' },
				{ href: '/tools', label: 'Tools' },
				{ href: '/config', label: 'Config' },
			],
		},
	];

	function resolveCrumb(pathname: string): { group: string; label: string } {
		for (const group of navGroups) {
			for (const item of group.items) {
				if (item.href === pathname) {
					return { group: group.title, label: item.label };
				}
			}
		}
		for (const group of navGroups) {
			for (const item of group.items) {
				if (item.href !== '/' && pathname.startsWith(item.href)) {
					return { group: group.title, label: item.label };
				}
			}
		}
		return { group: 'CCE', label: 'Untitled' };
	}

	let { children }: { children: any } = $props();

	let currentPage = $derived(page.url.pathname);
	let crumb = $derived(resolveCrumb(currentPage));
	let entityName = $derived(
		currentPage.startsWith('/entities/') && page.params.id
			? `Entity ${page.params.id}`
			: crumb.label
	);

	let mobileOpen = $state(false);
	let clock = $state('');
	let clockTimer: ReturnType<typeof setInterval> | null = null;

	let serverOk = $derived($metricsState.lastUpdated != null && !$metricsState.error);

	function toggleMobile() {
		mobileOpen = !mobileOpen;
		if (typeof document !== 'undefined') {
			document.body.style.overflow = mobileOpen ? 'hidden' : '';
		}
	}

	function closeMobile() {
		mobileOpen = false;
		if (typeof document !== 'undefined') {
			document.body.style.overflow = '';
		}
	}

	function onProjectChange(event: Event) {
		const target = event.currentTarget as HTMLSelectElement;
		currentProjectId.set(Number(target.value));
	}

	onMount(() => {
		loadProjects();
		healthActions.startAutoRefresh(15000);
		metricsActions.startAutoRefresh(30000);
		clock = new Date().toLocaleTimeString();
		clockTimer = setInterval(() => {
			clock = new Date().toLocaleTimeString();
		}, 1000);
	});

	onDestroy(() => {
		healthActions.stopAutoRefresh();
		metricsActions.stopAutoRefresh();
		if (clockTimer) {
			clearInterval(clockTimer);
		}
	});
</script>

<svelte:head>
	<title>Code Context Engine</title>
	<meta name="description" content="Web interface for Code Context Engine" />
</svelte:head>

<div class="app" class:nav-open={mobileOpen}>
	<aside class="sidebar">
		<a href="/" class="brand" onclick={closeMobile}>CCE<span>Console</span></a>

		<div class="status-block">
			<div class="status-row">
				<span class="status-dot" class:ok={serverOk} class:bad={!serverOk}></span>
				<span class="status-text">{serverOk ? 'Server Online' : 'Server Offline'}</span>
			</div>
			<label class="project-label" for="project-select">Current Project</label>
			<select
				id="project-select"
				class="project-select"
				onchange={onProjectChange}
			>
				{#if $projects.length === 0}
					<option value={$currentProjectId} selected>{`#${$currentProjectId}`}</option>
				{:else}
					{#each $projects as project (project.id)}
						<option value={Number(project.id)} selected={Number(project.id) === $currentProjectId}>
							{project.name || `#${project.id}`}
						</option>
					{/each}
				{/if}
			</select>
		</div>

		<nav class="nav" aria-label="Main navigation">
			{#each navGroups as group}
				<div class="nav-group">
					<div class="nav-title">{group.title}</div>
					{#each group.items as item}
						<a
							href={item.href}
							class="nav-item"
							class:active={item.href === '/' ? currentPage === '/' : currentPage.startsWith(item.href)}
							onclick={closeMobile}
						>
							{item.label}
						</a>
					{/each}
				</div>
			{/each}
		</nav>

		<div class="sidebar-footer">
			<span class="version">v0.1.0</span>
			{#if !isOnline}
				<span class="offline">⚠ Offline</span>
			{/if}
		</div>
	</aside>

	{#if mobileOpen}
		<div
			class="overlay"
			onclick={closeMobile}
			onkeydown={(e) => {
				if (e.key === 'Escape') closeMobile();
			}}
			role="button"
			tabindex="0"
			aria-label="Close menu"
		></div>
	{/if}

	<div class="main">
		<header class="topbar">
			<button
				class="menu-toggle"
				onclick={toggleMobile}
				aria-label="Toggle navigation menu"
				aria-expanded={mobileOpen}
			>
				<span class="hamburger"></span>
			</button>

			<nav class="breadcrumb" aria-label="Breadcrumb">
				<span class="crumb-group">{crumb.group}</span>
				<span class="crumb-sep">/</span>
				<span class="crumb-page">{entityName}</span>
			</nav>

			<div class="topbar-right">
				<div class="health-dots" title="Storage component health">
					{#if $metricsState.storageStatus}
						{@const s = $metricsState.storageStatus}
						<span class="hdot" class:ok={s.vector_storage.connected} title="Vector DB"></span>
						<span class="hdot" class:ok={s.bm25_storage.connected} title="BM25"></span>
						<span class="hdot" class:ok={s.relation_storage.connected} title="Relations"></span>
						<span class="hdot" class:ok={s.cache_storage.connected} title="Cache"></span>
					{:else}
						<span class="hdot idle"></span>
						<span class="hdot idle"></span>
						<span class="hdot idle"></span>
						<span class="hdot idle"></span>
					{/if}
				</div>
				<span class="clock">{clock}</span>
			</div>
		</header>

		<main class="content">
			<a href="#main-content" class="skip-link">Skip to main content</a>
			<div id="main-content">
				{@render children?.()}
			</div>
		</main>
	</div>

	<ToastContainer />
</div>

<style>
	.app {
		min-height: 100vh;
		display: flex;
	}

	/* ─── Sidebar ────────────────────────────────────────────── */
	.sidebar {
		width: 240px;
		flex-shrink: 0;
		background: var(--black);
		color: var(--white);
		position: sticky;
		top: 0;
		height: 100vh;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		padding: 1.5rem 1.25rem;
	}

	.brand {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.35rem;
		font-weight: 700;
		letter-spacing: -0.03em;
		color: var(--white);
		text-decoration: none;
	}

	.brand span {
		color: var(--accent);
	}

	.status-block {
		border: 1px solid var(--gray-700);
		padding: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.status-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.status-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--gray-500);
	}

	.status-dot.ok {
		background: var(--success);
		box-shadow: 0 0 0 3px rgba(27, 122, 61, 0.25);
	}

	.status-dot.bad {
		background: var(--danger);
		box-shadow: 0 0 0 3px rgba(198, 40, 40, 0.25);
	}

	.status-text {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-300);
	}

	.project-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
	}

	.project-select {
		width: 100%;
		padding: 0.5rem 0.5rem;
		background: var(--gray-900);
		color: var(--white);
		border: 1px solid var(--gray-700);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}

	.project-select:focus {
		outline: none;
		border-color: var(--accent);
	}

	.nav {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		flex: 1;
	}

	.nav-group {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.nav-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.15em;
		color: var(--gray-500);
		padding: 0 0.5rem 0.5rem;
	}

	.nav-item {
		display: block;
		padding: 0.5rem 0.75rem;
		color: var(--gray-300);
		text-decoration: none;
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		border-left: 2px solid transparent;
		transition: all 0.2s;
	}

	.nav-item:hover {
		color: var(--white);
		background: var(--gray-900);
	}

	.nav-item.active {
		color: var(--white);
		border-left-color: var(--accent);
		background: var(--gray-900);
	}

	.sidebar-footer {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-800);
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--gray-500);
	}

	.sidebar-footer .offline {
		color: var(--danger);
	}

	/* ─── Main ────────────────────────────────────────────────── */
	.main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.topbar {
		position: sticky;
		top: 0;
		z-index: 5;
		height: 58px;
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0 1.5rem;
		background: var(--white);
		border-bottom: 1px solid var(--black);
	}

	.breadcrumb {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.crumb-group {
		color: var(--gray-500);
	}

	.crumb-sep {
		color: var(--gray-300);
	}

	.crumb-page {
		color: var(--black);
		font-weight: 700;
	}

	.topbar-right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.health-dots {
		display: flex;
		gap: 0.35rem;
	}

	.hdot {
		width: 9px;
		height: 9px;
		border-radius: 50%;
		background: var(--danger);
	}

	.hdot.ok {
		background: var(--success);
	}

	.hdot.idle {
		background: var(--gray-300);
	}

	.clock {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}

	.content {
		flex: 1;
		padding: 2rem clamp(1rem, 3vw, 2.5rem);
		overflow-x: hidden;
	}

	.skip-link {
		position: absolute;
		top: -40px;
		left: 0;
		background: var(--accent);
		color: var(--white);
		padding: 8px 16px;
		z-index: 10000;
		transition: top 0.3s;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-decoration: none;
	}

	.skip-link:focus {
		top: 0;
	}

	/* ─── Mobile drawer ──────────────────────────────────────── */
	.menu-toggle {
		display: none;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		background: none;
		border: 1px solid var(--black);
		cursor: pointer;
	}

	.hamburger {
		position: relative;
		width: 20px;
		height: 2px;
		background: var(--black);
	}

	.hamburger::before,
	.hamburger::after {
		content: '';
		position: absolute;
		width: 20px;
		height: 2px;
		background: var(--black);
	}

	.hamburger::before {
		top: -6px;
	}

	.hamburger::after {
		top: 6px;
	}

	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		z-index: 40;
	}

	@media (max-width: 900px) {
		.sidebar {
			position: fixed;
			left: -100%;
			top: 0;
			z-index: 50;
			transition: left 0.3s ease;
			box-shadow: 2px 0 10px rgba(0, 0, 0, 0.3);
		}

		.app.nav-open .sidebar {
			left: 0;
		}

		.menu-toggle {
			display: flex;
		}
	}
</style>
