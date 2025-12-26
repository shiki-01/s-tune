import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import masterCSS from '@master/css.vite';
import vitePluginWasmPack from '@evankirkiles/vite-plugin-wasm-pack';

export default defineConfig(({ command }) => {
	return {
		plugins: [
			sveltekit(),
			masterCSS(),
			...(command === 'build' ? [vitePluginWasmPack('./melody-dsp')] : [])
		],
		server: {
			fs: {
				allow: ['./master.css.ts', './melody-dsp/pkg']
			}
		}
	};
});
