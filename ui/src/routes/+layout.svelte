<script lang="ts">
	import type { Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();
</script>

{@render children()}

<style>
	:global(:root) {
		--bg: #f7f7f5;
		--fg: #1c1c1a;
		--muted: #6b6b66;
		--surface: #ffffff;
		--border: #deded9;
		--accent: #3a5bd9;
		font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
		line-height: 1.5;
		-webkit-text-size-adjust: 100%;
	}

	@media (prefers-color-scheme: dark) {
		:global(:root) {
			--bg: #161615;
			--fg: #ededea;
			--muted: #9a9a94;
			--surface: #20201e;
			--border: #33332f;
			--accent: #8ea2ff;
		}
	}

	:global(body) {
		margin: 0;
		background: var(--bg);
		color: var(--fg);
		/* Keep content clear of notches and home indicators on mobile. */
		padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
			env(safe-area-inset-left);
	}
</style>
