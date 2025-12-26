import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import masterCSS from '@master/css.vite';
import vitePluginWasmPack from '@evankirkiles/vite-plugin-wasm-pack';

export default defineConfig({
	plugins: [
		sveltekit(),
		masterCSS(),
		vitePluginWasmPack('./melody-dsp'),
	],
	server: {
		fs: {
			allow: [
				"./master.css.ts"
			]
		}
	}
});
