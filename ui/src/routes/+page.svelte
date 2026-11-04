<script lang="ts">
	// The Overview tab: what subscriptions cost, what's due soon, and month by
	// month.
	import { onMount } from 'svelte';
	import { getOverview } from '#lib/api.ts';
	import type { Overview } from '#lib/bindings/Overview.ts';
	import MonthlyChart from '#lib/components/MonthlyChart.svelte';
	import TotalCard from '#lib/components/TotalCard.svelte';
	import UpcomingPayments from '#lib/components/UpcomingPayments.svelte';
	import { view } from '#lib/view.svelte.ts';

	let overview = $state<Overview | null>(null);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			overview = await getOverview();
		} catch (e) {
			error = String(e);
		}
	});
</script>

<main>
	<header>
		<h1>Epicycle</h1>
	</header>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if overview}
		<TotalCard
			monthlyByCurrency={overview.monthlyTotal}
			reloadKey={overview}
			bind:currency={view.currency}
			bind:ratesVersion={view.ratesVersion}
		/>

		<UpcomingPayments today={overview.today} reloadKey={overview} />

		<MonthlyChart currency={view.currency} reloadKey={[overview, view.ratesVersion]} />
	{:else if !error}
		<p class="muted">Loading…</p>
	{/if}
</main>

<style>
	main {
		max-width: 44rem;
		margin: 0 auto;
		padding: 1rem;
		display: grid;
		gap: 1.25rem;
	}

	/* Grid items default to their content's minimum width, which would let a
	   wide card (or the chart) push the column past a phone's screen. */
	main > :global(*) {
		min-width: 0;
	}

	h1 {
		margin: 0;
		font-size: 1.5rem;
	}

	p {
		margin: 0;
	}

	.muted {
		color: var(--muted);
	}

	.error {
		color: var(--danger);
	}
</style>
