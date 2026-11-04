<script lang="ts">
	// Exchange rates: the ECB's, fetched automatically when online, and rates
	// entered by hand for currencies the ECB doesn't publish or while offline.
	import { onMount } from 'svelte';
	import {
		addManualRate,
		deleteManualRate,
		getCurrencies,
		getRates,
		onRatesUpdated,
		refreshRates
	} from '#lib/api.ts';
	import type { CurrencyOption } from '#lib/bindings/CurrencyOption.ts';
	import type { RatesOverview } from '#lib/bindings/RatesOverview.ts';
	import CurrencyInput from '#lib/components/CurrencyInput.svelte';
	import DateInput from '#lib/components/DateInput.svelte';
	import { describeRateSource, formatRate } from '#lib/format.ts';

	// `undefined` means the device's locale.
	const locale = undefined;

	let rates = $state<RatesOverview | null>(null);
	let options = $state<CurrencyOption[]>([]);
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);
	let busy = $state(false);

	// Manual rate form.
	let currency = $state('');
	let date = $state('');
	let perEur = $state('');

	// EUR is the base and has no rate.
	const rateOptions = $derived(options.filter((o) => o.code !== 'EUR'));

	onMount(async () => {
		try {
			[rates, options] = await Promise.all([getRates(), getCurrencies()]);
			date = rates.today;
		} catch (e) {
			error = String(e);
		}
	});

	// New rates fetched in the background while this screen is open.
	$effect(() =>
		onRatesUpdated(async () => {
			rates = await getRates();
		})
	);

	async function run(action: () => Promise<void>) {
		busy = true;
		error = null;
		message = null;
		try {
			await action();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	const update = () =>
		run(async () => {
			const stored = await refreshRates();
			rates = await getRates();
			message = stored > 0 ? 'Rates updated.' : 'Rates are already up to date.';
		});

	function add(event: SubmitEvent) {
		event.preventDefault();
		run(async () => {
			rates = await addManualRate(currency, date, perEur);
			message = `Saved 1 EUR = ${perEur} ${currency}.`;
			perEur = '';
		});
	}

	const remove = (code: string, day: string) =>
		run(async () => {
			rates = await deleteManualRate(code, day);
		});
</script>

<main>
	<nav>
		<a href="/more" class="back">‹ More</a>
	</nav>
	<h1>Exchange rates</h1>
	<p class="muted">
		Totals in one currency use the European Central Bank's daily reference rates. While the app is
		open and online, it checks for new ones every hour; the ECB publishes them on working days
		around 16:00 CET. For a currency the ECB doesn't publish, or to use your own rate, enter
		one below: it takes precedence over the ECB's for the same day.
	</p>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}
	{#if message}
		<p role="status">{message}</p>
	{/if}

	{#if rates}
		<section aria-labelledby="current-heading">
			<div class="head">
				<h2 id="current-heading">Current rates</h2>
				<button class="secondary" onclick={update} disabled={busy}>Update now</button>
			</div>
			{#if rates.current.length === 0}
				<p class="muted">No rates yet. Connect to the internet or enter one below.</p>
			{:else}
				<ul class="card">
					{#each rates.current as rate (rate.currency)}
						<li>
							<span>{formatRate(rate, locale)}</span>
							<span class:stale={rate.freshness === 'stale'} class="muted">
								{describeRateSource(rate, locale)}
							</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section aria-labelledby="manual-heading">
			<h2 id="manual-heading">Enter a rate</h2>
			<form class="card form" onsubmit={add}>
				<div class="row">
					<span class="eq">1 EUR =</span>
					<label class="grow">
						Amount
						<input bind:value={perEur} required inputmode="decimal" autocomplete="off" placeholder="5.0" />
					</label>
					<div class="currency">
						<CurrencyInput bind:value={currency} options={rateOptions} label="Currency" />
					</div>
				</div>
				<DateInput bind:value={date} label="Applies from" required today={rates.today} />
				<div class="actions">
					<button type="submit" disabled={busy}>Save rate</button>
				</div>
			</form>

			{#if rates.manual.length > 0}
				<h3>Your rates</h3>
				<ul class="card">
					{#each rates.manual as rate (rate.currency + rate.date)}
						<li class="manual">
							<span>
								{formatRate(rate, locale)}
								<span class="muted">({describeRateSource(rate, locale)})</span>
							</span>
							<button class="link" onclick={() => remove(rate.currency, rate.date)} disabled={busy}>
								Delete
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
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

	.back {
		display: inline-flex;
		align-items: center;
		min-height: 44px;
		color: var(--accent);
		text-decoration: none;
		font-weight: 500;
	}

	h1 {
		margin: 0;
		font-size: 1.5rem;
	}

	h2 {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
	}

	h3 {
		margin: 1rem 0 0.5rem;
		font-size: 0.875rem;
		color: var(--muted);
	}

	p {
		margin: 0;
	}

	.muted {
		color: var(--muted);
	}

	.stale {
		color: var(--danger);
	}

	.error {
		color: var(--danger);
	}

	.head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		margin-bottom: 0.5rem;
	}

	.head h2 {
		margin: 0;
	}

	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	ul.card {
		list-style: none;
		margin: 0;
		display: grid;
		gap: 0.5rem;
	}

	li {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 0.25rem 1rem;
	}

	li.manual {
		align-items: center;
	}

	.form {
		display: grid;
		gap: 1rem;
	}

	.row {
		display: flex;
		gap: 0.75rem;
		align-items: start;
	}

	.eq {
		padding-top: 2.4rem;
		font-weight: 600;
		white-space: nowrap;
	}

	label {
		display: grid;
		gap: 0.375rem;
		font-weight: 500;
	}

	.grow {
		flex: 1;
		min-width: 0;
	}

	.currency {
		width: 8rem;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
	}
</style>
