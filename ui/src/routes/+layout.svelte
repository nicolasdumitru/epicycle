<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { tabFor, tabs } from '#lib/tabs.ts';

	let { children }: { children: Snippet } = $props();

	const current = $derived(tabFor(page.url.pathname));
</script>

<!-- First in the page so keyboard and screen-reader users meet it first; on
     phones CSS moves it to the bottom of the screen. -->
<nav class="tabs" aria-label="Sections">
	{#each tabs as tab (tab.href)}
		<a href={tab.href} aria-current={current === tab.href ? 'page' : undefined}>
			<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
				{#if tab.href === '/'}
					<!-- Overview: columns of a chart. -->
					<path d="M5 20V12M12 20V5M19 20v-9" />
				{:else if tab.href === '/subscriptions'}
					<!-- Subscriptions: a list. -->
					<path d="M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01" />
				{:else}
					<!-- More: three dots. -->
					<path d="M5 12h.01M12 12h.01M19 12h.01" />
				{/if}
			</svg>
			<span>{tab.label}</span>
		</a>
	{/each}
</nav>

<div class="content">
	{@render children()}
</div>

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

	/* Tabs: a bar along the bottom on phones, as in most mobile apps. */
	.tabs {
		position: fixed;
		z-index: 20;
		left: 0;
		right: 0;
		bottom: 0;
		display: flex;
		justify-content: space-around;
		padding: 0.25rem env(safe-area-inset-right) env(safe-area-inset-bottom)
			env(safe-area-inset-left);
		background: var(--surface);
		border-top: 1px solid var(--border);
	}

	.tabs a {
		flex: 1;
		max-width: 10rem;
		display: grid;
		justify-items: center;
		gap: 0.125rem;
		min-height: 52px;
		padding: 0.375rem 0.25rem;
		color: var(--muted);
		font-size: 0.75rem;
		font-weight: 600;
		text-decoration: none;
		border-radius: 0.5rem;
	}

	.tabs a[aria-current='page'] {
		color: var(--accent);
	}

	.tabs svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 2.25;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	/* Room for the bar, so it never covers the end of a page. */
	.content {
		padding-bottom: calc(64px + env(safe-area-inset-bottom));
	}

	/* Wider screens: a row of tabs at the top instead. */
	@media (min-width: 48rem) {
		.tabs {
			position: sticky;
			top: 0;
			bottom: auto;
			justify-content: center;
			gap: 0.5rem;
			padding: 0.5rem 1rem;
			border-top: 0;
			border-bottom: 1px solid var(--border);
		}

		.tabs a {
			flex: none;
			max-width: none;
			display: flex;
			align-items: center;
			gap: 0.5rem;
			min-height: 44px;
			padding: 0 1rem;
			font-size: 0.9375rem;
		}

		.tabs a[aria-current='page'] {
			background: var(--bg);
		}

		.content {
			padding-bottom: 0;
		}
	}

	:global(:focus-visible) {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
</style>
