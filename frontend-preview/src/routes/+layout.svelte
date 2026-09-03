<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import ToastContainer from '$lib/components/ui/ToastContainer.svelte';
	import { isOnline } from '$lib/stores/network';

	let { children }: { children: any } = $props();

	let currentPage = $derived(page.url.pathname);
	let mobileMenuOpen = $state(false);

	function toggleMobileMenu() {
		mobileMenuOpen = !mobileMenuOpen;
		// Prevent body scroll when menu is open
		if (typeof document !== 'undefined') {
			document.body.style.overflow = mobileMenuOpen ? 'hidden' : '';
		}
	}

	function closeMobileMenu() {
		mobileMenuOpen = false;
		if (typeof document !== 'undefined') {
			document.body.style.overflow = '';
		}
	}
</script>

<svelte:head>
	<title>Code Context Engine</title>
	<meta name="description" content="Web interface for Code Context Engine" />
</svelte:head>

<div class="app">
	<header class="header">
		<div class="container">
			<div class="header-inner">
				<div class="logo">
					CCE<span>Frontend</span>
				</div>
				
				<!-- Mobile Menu Toggle -->
				<button 
					class="mobile-menu-toggle" 
					onclick={toggleMobileMenu}
					aria-label="Toggle navigation menu"
					aria-expanded={mobileMenuOpen}
				>
					<span class="hamburger-icon"></span>
				</button>
				
				<nav class="nav" class:open={mobileMenuOpen} aria-label="Main navigation">
					<a href="/" class="nav-link" class:active={currentPage === '/'} onclick={closeMobileMenu}>Dashboard</a>
					<a href="/index" class="nav-link" class:active={currentPage.startsWith('/index')} onclick={closeMobileMenu}>Index</a>
					<a href="/search" class="nav-link" class:active={currentPage.startsWith('/search')} onclick={closeMobileMenu}>Search</a>
					<a href="/entities" class="nav-link" class:active={currentPage.startsWith('/entities')} onclick={closeMobileMenu}>Entities</a>
					<a href="/storage" class="nav-link" class:active={currentPage.startsWith('/storage')} onclick={closeMobileMenu}>Storage</a>
					<a href="/watch" class="nav-link" class:active={currentPage.startsWith('/watch')} onclick={closeMobileMenu}>Watch</a>
					<a href="/tools" class="nav-link" class:active={currentPage.startsWith('/tools')} onclick={closeMobileMenu}>Tools</a>
				</nav>
				
				<!-- Mobile Menu Overlay -->
				{#if mobileMenuOpen}
					<div 
						class="mobile-overlay" 
						onclick={closeMobileMenu}
						onkeydown={(e) => {
							if (e.key === 'Escape') closeMobileMenu();
						}}
						role="button"
						tabindex="0"
						aria-label="Close menu"
					></div>
				{/if}
				
				<div class="header-meta">
					{#if !isOnline}
						<span class="offline-indicator" title="You are offline">⚠ Offline</span>
					{/if}
					v0.1.0
				</div>
			</div>
		</div>
	</header>

	<main class="main">
		<a href="#main-content" class="skip-link">Skip to main content</a>
		<div id="main-content">
			{@render children?.()}
		</div>
	</main>

	<ToastContainer />

	<footer class="footer">
		<div class="container">
			<div class="footer-inner">
				<div class="footer-left">
					Code Context Engine Frontend
				</div>
				<div class="footer-links">
					<a href="https://github.com/kkkqxk123" target="_blank" rel="noopener noreferrer">GitHub</a>
					<a href="/config" class="nav-link">Config</a>
				</div>
				<div class="footer-right">
					Built with SvelteKit
				</div>
			</div>
		</div>
	</footer>
</div>

<style>
	.app {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
	}

	.header {
		padding: 2rem 0;
		border-bottom: 1px solid var(--black);
		background: var(--white);
	}

	.header-inner {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: center;
		gap: 2rem;
	}

	.logo {
		font-size: 1.5rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.logo span {
		color: var(--gray-400);
	}

	.nav {
		display: flex;
		gap: 2rem;
		justify-content: center;
	}

	.nav-link {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		color: var(--black);
		text-decoration: none;
		position: relative;
		padding-bottom: 0.25rem;
	}

	.nav-link::after {
		content: '';
		position: absolute;
		bottom: 0;
		left: 0;
		width: 100%;
		height: 1px;
		background: var(--black);
		transform: scaleX(0);
		transition: transform 0.3s;
	}

	.nav-link:hover::after,
	.nav-link.active::after {
		transform: scaleX(1);
	}

	.header-meta {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-align: right;
		color: var(--gray-600);
	}

	.main {
		flex: 1;
	}

	.footer {
		padding: 2rem 0;
		border-top: 1px solid var(--black);
		background: var(--white);
	}

	.footer-inner {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: center;
		gap: 2rem;
	}

	.footer-left {
		font-size: 0.85rem;
		color: var(--gray-600);
	}

	.footer-links {
		display: flex;
		gap: 2rem;
		justify-content: center;
	}

	.footer-links a {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		color: var(--black);
		text-decoration: none;
	}

	.footer-links a:hover {
		color: var(--accent);
	}

	.footer-right {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
		text-align: right;
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

	.offline-indicator {
		color: var(--danger);
		font-weight: 600;
		margin-right: 0.5rem;
	}

	@media (max-width: 1024px) {
		.header-inner {
			grid-template-columns: 1fr;
			gap: 1rem;
		}

		.nav {
			order: -1;
		}

		.header-meta {
			text-align: center;
		}

		.footer-inner {
			grid-template-columns: 1fr;
			text-align: center;
		}

		.footer-right {
			text-align: center;
		}
	}

	@media (max-width: 768px) {
		.header-inner {
			grid-template-columns: auto 1fr auto;
			gap: 1rem;
		}

		.logo {
			font-size: 1.25rem;
		}

		/* Hide desktop nav */
		.nav {
			position: fixed;
			top: 0;
			left: -100%;
			width: 80%;
			max-width: 300px;
			height: 100vh;
			background: var(--white);
			flex-direction: column;
			gap: 0;
			padding: 5rem 2rem 2rem;
			transition: left 0.3s ease;
			z-index: 999;
			box-shadow: 2px 0 10px rgba(0, 0, 0, 0.1);
		}

		.nav.open {
			left: 0;
		}

		.nav-link {
			padding: 1rem 0;
			border-bottom: 1px solid var(--gray-200);
			font-size: 1rem;
			min-height: 44px;
			display: flex;
			align-items: center;
		}

		.nav-link::after {
			display: none;
		}

		.header-meta {
			display: none;
		}

		/* Mobile menu toggle button */
		.mobile-menu-toggle {
			display: flex;
			align-items: center;
			justify-content: center;
			width: 44px;
			height: 44px;
			background: none;
			border: 1px solid var(--black);
			cursor: pointer;
			transition: background 0.3s;
			min-width: 44px;
			min-height: 44px;
		}

		.mobile-menu-toggle:hover {
			background: var(--gray-100);
		}

		.hamburger-icon {
			position: relative;
			width: 24px;
			height: 2px;
			background: var(--black);
			transition: all 0.3s;
		}

		.hamburger-icon::before,
		.hamburger-icon::after {
			content: '';
			position: absolute;
			width: 24px;
			height: 2px;
			background: var(--black);
			transition: all 0.3s;
		}

		.hamburger-icon::before {
			top: -8px;
		}

		.hamburger-icon::after {
			top: 8px;
		}

		/* Hamburger animation when open */
		.mobile-menu-toggle[aria-expanded="true"] .hamburger-icon {
			background: transparent;
		}

		.mobile-menu-toggle[aria-expanded="true"] .hamburger-icon::before {
			transform: rotate(45deg);
			top: 0;
		}

		.mobile-menu-toggle[aria-expanded="true"] .hamburger-icon::after {
			transform: rotate(-45deg);
			top: 0;
		}

		.footer-inner {
			grid-template-columns: 1fr;
			text-align: center;
			gap: 1rem;
		}

		.footer-right {
			text-align: center;
		}
		
		/* Mobile overlay */
		.mobile-overlay {
			position: fixed;
			top: 0;
			left: 0;
			right: 0;
			bottom: 0;
			background: rgba(0, 0, 0, 0.5);
			z-index: 998;
			animation: fadeIn 0.3s ease;
		}
	}

	@media (min-width: 769px) {
		.mobile-menu-toggle {
			display: none;
		}
	}
</style>
