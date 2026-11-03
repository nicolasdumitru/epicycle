<script lang="ts">
	// The current recurring commitment converted to one currency: the default
	// (reporting) currency, or any other chosen here. Shows which rates the
	// total relies on and how old they are, and recomputes when the app stores
	// new ECB rates in the background.
	import { onMount } from 'svelte';
	import {
		getConvertedTotal,
		getCurrencies,
		getFavorites,
		onRatesUpdated,
		setFavorites,
		setReportingCurrency
	} from '#lib/api.ts';
	import type { ConvertedTotal } from '#lib/bindings/ConvertedTotal.ts';
	import type { CurrencyOption } from '#lib/bindings/CurrencyOption.ts';
	import type { MoneyView } from '#lib/bindings/MoneyView.ts';
	import { describeRateSource, formatMoney, formatRate } from '#lib/format.ts';
	import CurrencyInput from './CurrencyInput.svelte';
	import FavoriteChips from './FavoriteChips.svelte';

	let {
		monthlyByCurrency,
		reloadKey,
		currency = $bindable(),
		ratesVersion = $bindable()
	}: {
		/** The commitment in each original currency. */
		monthlyByCurrency: MoneyView[];
		/** Anything that changes when subscriptions do, to recompute. */
		reloadKey: unknown;
		/** The currency totals are shown in; empty until the default loads. */
		currency: string;
		/** Goes up whenever exchange rates or the default currency change. */
		ratesVersion: number;
	} = $props();

	// `undefined` means the device's locale.
	const locale = undefined;

	let options = $state<CurrencyOption[]>([]);
	let favorites = $state<string[]>([]);
	let total = $state<ConvertedTotal | null>(null);
	let error = $state<string | null>(null);

	async function load(code: string) {
		try {
			const result = await getConvertedTotal(code);
			// Ignore an answer for a currency that is no longer chosen.
			if (code === currency) total = result;
			error = null;
		} catch (e) {
			error = String(e);
		}
	}

	$effect(() => {
		void reloadKey;
		void ratesVersion;
		if (currency) load(currency);
	});

	onMount(async () => {
		try {
			[options, favorites] = await Promise.all([getCurrencies(), getFavorites()]);
			currency = (await getConvertedTotal(null)).defaultCurrency;
		} catch (e) {
			error = String(e);
		}
	});

	$effect(() => onRatesUpdated(() => (ratesVersion += 1)));

	const isFavorite = $derived(favorites.includes(currency));

	async function toggleFavorite() {
		if (!currency) return;
		try {
			favorites = await setFavorites(
				isFavorite ? favorites.filter((c) => c !== currency) : [...favorites, currency]
			);
		} catch (e) {
			error = String(e);
		}
	}

	async function makeDefault() {
		if (!total) return;
		try {
			await setReportingCurrency(total.currency);
			ratesVersion += 1;
		} catch (e) {
			error = String(e);
		}
	}

	const stale = $derived(total?.rates.filter((r) => r.freshness === 'stale') ?? []);
	// Worth showing only when something was actually converted.
	const others = $derived(monthlyByCurrency.filter((m) => m.currency !== total?.currency));
</script>

<section class="card" aria-labelledby="total-heading">
	<div class="head">
		<h2 id="total-heading">Recurring commitment</h2>
		<div class="picker">
			<CurrencyInput bind:value={currency} {options} label="Shown in" />
			<button
				type="button"
				class="star"
				class:on={isFavorite}
				onclick={toggleFavorite}
				disabled={!currency}
				aria-pressed={isFavorite}
				aria-label={isFavorite
					? `Remove ${currency} from favourites`
					: `Add ${currency} to favourites`}
				title={isFavorite ? 'Remove from favourites' : 'Add to favourites'}
			>
				{isFavorite ? '★' : '☆'}
			</button>
		</div>
	</div>

	<FavoriteChips {favorites} bind:value={currency} label="Show totals in" />

	{#if total}
		{#if total.missing.length > 0}
			<p class="warning">
				No exchange rate for {total.missing.join(', ')}, so the total can't be shown.
				<a href="/rates">Enter a rate</a>
			</p>
		{:else}
			<div class="stats">
				<div>
					<h3>Per month</h3>
					<p class="value">{total.monthly ? formatMoney(total.monthly, locale) : '–'}</p>
				</div>
				<div>
					<h3>Per year</h3>
					<p class="value">{total.annual ? formatMoney(total.annual, locale) : '–'}</p>
				</div>
			</div>
		{/if}

		{#if others.length > 0}
			<p class="muted">
				Per month in original currencies:
				{monthlyByCurrency.map((m) => formatMoney(m, locale)).join(' + ')}
			</p>
		{/if}

		{#if stale.length > 0}
			<p class="warning">
				Some rates are out of date: the app updates them when it's online.
				<a href="/rates">Check rates</a>
			</p>
		{/if}

		{#if total.rates.length > 0}
			<details>
				<summary>Exchange rates used</summary>
				<ul>
					{#each total.rates as rate (rate.currency)}
						<li>{formatRate(rate, locale)} <span class="muted">({describeRateSource(rate, locale)})</span></li>
					{/each}
				</ul>
				<a href="/rates">All rates</a>
			</details>
		{/if}

		{#if total.currency !== total.defaultCurrency}
			<button class="link" onclick={makeDefault}>Always show totals in {total.currency}</button>
		{/if}

		<p class="note">
			What active subscriptions cost at current prices, averaged per month and per year.
		</p>
	{:else if !error}
		<p class="muted">Loading…</p>
	{/if}

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}
</section>

<style>
	.card {
		display: grid;
		gap: 0.75rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: start;
		gap: 0.5rem 1rem;
	}

	h2 {
		margin: 0;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
	}

	.picker {
		display: flex;
		align-items: end;
		gap: 0.375rem;
		flex: none;
	}

	.picker > :global(:first-child) {
		width: 8rem;
	}

	/* Sits level with the input, below its label. */
	.star {
		width: 44px;
		padding: 0;
		margin-bottom: 1.6rem;
		background: transparent;
		border-color: var(--border);
		color: var(--muted);
		font-size: 1.25rem;
	}

	.star.on {
		color: var(--accent);
		border-color: var(--accent);
	}

	.stats {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.75rem;
	}

	h3 {
		margin: 0 0 0.25rem;
		font-size: 0.875rem;
		color: var(--muted);
		font-weight: 600;
	}

	p {
		margin: 0;
	}

	.value {
		font-size: 1.75rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
	}

	.muted,
	.note {
		color: var(--muted);
	}

	.note {
		font-size: 0.875rem;
	}

	.warning {
		color: var(--danger);
	}

	.error {
		color: var(--danger);
	}

	a {
		color: var(--accent);
	}

	details ul {
		margin: 0.5rem 0;
		padding-left: 1.25rem;
	}

	summary {
		cursor: pointer;
		padding: 0.625rem 0;
	}

	button.link {
		justify-self: start;
		min-height: 44px;
	}
</style>
