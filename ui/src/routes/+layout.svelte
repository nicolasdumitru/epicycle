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
		--accent-fg: #ffffff;
		--danger: #c0362c;
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
			--accent-fg: #10142a;
			--danger: #ff7b6e;
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

	/* Form controls: at least 44px tall for touch, and 16px text so iOS doesn't zoom in. */
	:global(input),
	:global(select),
	:global(button) {
		font: inherit;
		font-size: max(1rem, 16px);
		min-height: 44px;
		box-sizing: border-box;
		border-radius: 0.5rem;
	}

	:global(input),
	:global(select) {
		width: 100%;
		padding: 0 0.75rem;
		color: var(--fg);
		background: var(--bg);
		border: 1px solid var(--border);
	}

	:global(button) {
		padding: 0 1rem;
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--accent-fg);
		font-weight: 600;
		cursor: pointer;
	}

	:global(button:disabled) {
		opacity: 0.6;
		cursor: default;
	}

	:global(button.secondary) {
		background: transparent;
		color: var(--fg);
		border-color: var(--border);
	}

	:global(button.danger) {
		background: var(--danger);
		border-color: var(--danger);
		color: var(--surface);
	}

	:global(button.link) {
		background: none;
		border: none;
		color: var(--muted);
		font-weight: 400;
		padding: 0 0.25rem;
		text-decoration: underline;
	}

	:global(:focus-visible) {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
</style>
