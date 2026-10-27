<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';

	type AppInfo = { version: string; os: string; arch: string };

	const info = invoke<AppInfo>('app_info');
</script>

<main>
	<h1>Epicycle</h1>
	<p class="muted">Prototype: checks that the toolchain, webview and IPC bridge work.</p>

	<section aria-live="polite">
		{#await info}
			<p>Contacting the Rust backend…</p>
		{:then { version, os, arch }}
			<dl>
				<dt>Version</dt>
				<dd>{version}</dd>
				<dt>Platform</dt>
				<dd>{os} / {arch}</dd>
			</dl>
		{:catch error}
			<p role="alert">IPC call failed: {String(error)}</p>
		{/await}
	</section>
</main>

<style>
	main {
		max-width: 40rem;
		margin: 0 auto;
		padding: 1.5rem 1rem;
	}

	.muted {
		color: var(--muted);
	}

	section {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.25rem 1rem;
		margin: 0;
	}

	dt {
		color: var(--muted);
	}

	dd {
		margin: 0;
	}
</style>
