import adapter from '@sveltejs/adapter-auto';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter()
	}
	// Note: SvelteKit automatically sets compilerOptions.dev based on build mode
	// No need to manually configure it unless you have special requirements
};

export default config;
