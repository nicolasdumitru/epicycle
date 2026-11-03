<script lang="ts">
	// One-tap buttons for the favourite currencies. Shows nothing if there are
	// no favourites yet.
	let {
		favorites,
		value = $bindable(),
		label
	}: {
		favorites: string[];
		/** The chosen currency code. */
		value: string;
		/** What the chips choose, for screen readers, e.g. "Show totals in". */
		label: string;
	} = $props();
</script>

{#if favorites.length > 0}
	<div class="chips" role="group" aria-label={label}>
		{#each favorites as code (code)}
			<button
				type="button"
				class="chip"
				class:selected={code === value}
				aria-pressed={code === value}
				onclick={() => (value = code)}>{code}</button
			>
		{/each}
	</div>
{/if}

<style>
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.chip {
		min-width: 4rem;
		padding: 0 0.75rem;
		background: transparent;
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 999px;
		font-weight: 600;
	}

	.chip.selected {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-fg);
	}
</style>
