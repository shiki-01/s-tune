import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import vitePluginWasmPack from '@evankirkiles/vite-plugin-wasm-pack';

export default defineConfig({
	plugins: [
		sveltekit(),
		vitePluginWasmPack('./melody-dsp'),
	]
});
