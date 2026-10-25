import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig(({ mode }) => {
	const isProd = mode === 'production';
	// Preview mode swaps the API client to the mock-backed implementation.
	// The default resolution goes through the tsconfig paths to client.ts,
	// so mock code never enters the module graph of default builds.
	const isPreview = mode === 'preview';

	return {
		plugins: [
			sveltekit(),
			isProd &&
				visualizer({
					open: false,
					filename: 'stats.md',
					emitFile: true,
					gzipSize: true,
					brotliSize: true,
					template: 'markdown',
				}),
		].filter(Boolean),
		resolve: {
			alias: isPreview
				? [
						{
							find: '#api/client',
							replacement: fileURLToPath(
								new URL('./src/lib/api/client.preview.ts', import.meta.url),
							),
						},
					]
				: [],
		},
		build: {
			sourcemap: !isProd,
			minify: isProd, // Use default minifier (esbuild for client, terser if needed)
		},
		server: {
			port: 3001,
			proxy: {
				'/api': {
					// Backend target for the dev proxy; override to test against a
					// remote backend, e.g. CCE_DEV_API_TARGET=http://10.0.0.5:9000
					target: process.env.CCE_DEV_API_TARGET ?? 'http://localhost:9000',
					changeOrigin: true,
				},
			},
		},
	};
});
