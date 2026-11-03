import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Set by the Tauri CLI when developing on a physical mobile device.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [
		sveltekit({
			// Tauri serves static files, so build a single-page app with no server rendering.
			adapter: adapter({ fallback: 'index.html' }),
			// The default version is the build time, which changes the output of every
			// build. It only matters for detecting new deployments of a website, which
			// an app bundled with Tauri never has.
			version: { name: 'epicycle' }
		})
	],
	// Keep Rust compiler errors visible in the terminal.
	clearScreen: false,
	server: {
		port: 5173,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined
	}
});
