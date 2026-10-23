import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig(({ mode }) => {
	const isProd = mode === 'production';

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
