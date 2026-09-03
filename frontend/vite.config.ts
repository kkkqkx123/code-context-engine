import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig(({ mode }) => {
	const isProd = mode === 'production';
	
	return {
		plugins: [
			sveltekit(),
			isProd && visualizer({
				open: false,
				filename: 'stats.html',
				gzipSize: true,
				brotliSize: true,
				template: 'treemap'
			})
		].filter(Boolean),
		build: {
			sourcemap: !isProd,
			minify: isProd, // Use default minifier (esbuild for client, terser if needed)
			rollupOptions: {
				output: {
					manualChunks(id) {
						// Extract Svelte core runtime
						if (id.includes('node_modules/svelte/')) {
							return 'svelte-core';
						}
						// Extract SvelteKit client runtime
						if (id.includes('node_modules/@sveltejs/kit/')) {
							return 'sveltekit-client';
						}
						// Extract UI components
						if (id.includes('/src/lib/components/ui/')) {
							return 'ui-components';
						}
						// Extract API utilities
						if (id.includes('/src/lib/api/')) {
							return 'api-utils';
						}
						// Extract stores
						if (id.includes('/src/lib/stores/')) {
							return 'stores';
						}
					}
				}
			}
		},
		server: {
			port: 3001,
			proxy: {
				'/api': {
					target: 'http://localhost:9000',
					changeOrigin: true
				}
			}
		}
	};
});
