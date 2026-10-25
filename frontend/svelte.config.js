import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({ fallback: 'index.html' }),
		// Default API client binding; preview mode overrides it via the Vite
		// resolve.alias in vite.config.ts (see docs/frontend/preview-design.md).
		alias: {
			'#api/client': 'src/lib/api/client',
		},
	},
	// Note: SvelteKit automatically sets compilerOptions.dev based on build mode
	// No need to manually configure it unless you have special requirements
};

export default config;
