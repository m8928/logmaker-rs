import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({
			pages: '../core/static',
			assets: '../core/static',
			fallback: 'index.html'
		})
	}
};
