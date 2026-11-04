<script lang="ts">
	// The amount per payment: exact by default, or, when it varies, a lower
	// and/or upper bound and an expected amount. The expected amount is the
	// average of both bounds, the one bound given, or entered by hand; the
	// form says which applies (docs/DECISIONS.md, "Estimated amounts").
	import type { EstimateInput } from '#lib/bindings/EstimateInput.ts';
	import type { ExpectedChoice } from '#lib/bindings/ExpectedChoice.ts';

	let {
		amount = $bindable(),
		estimate = $bindable(),
		currency,
		label = 'Price per payment'
	}: {
		/** The exact amount, or the expected one when entered by hand. */
		amount: string;
		/** Null for an exact amount. */
		estimate: EstimateInput | null;
		/** ISO code, for showing the expected amount; may be empty. */
		currency: string;
		label?: string;
	} = $props();

	// svelte-ignore state_referenced_locally (the starting values only)
	let varies = $state(estimate !== null);
	// svelte-ignore state_referenced_locally
	let low = $state(estimate?.low ?? '');
	// svelte-ignore state_referenced_locally
	let high = $state(estimate?.high ?? '');
	// svelte-ignore state_referenced_locally
	let byHand = $state(estimate?.expected === 'manual');

	const hasLow = $derived(low.trim() !== '');
	const hasHigh = $derived(high.trim() !== '');
	const rule = $derived<ExpectedChoice>(
		byHand ? 'manual' : hasLow && hasHigh ? 'midpoint' : 'bound'
	);

	// Report the estimate as the user edits it.
	$effect(() => {
		estimate = varies ? { low, high, expected: rule } : null;
	});

	/** A typed amount as a number, accepting a comma as decimal separator. */
	function number(text: string): number | null {
		const n = Number(text.trim().replace(',', '.'));
		return text.trim() === '' || Number.isNaN(n) ? null : n;
	}

	function money(value: number | null): string {
		if (value === null) return '…';
		if (!/^[A-Z]{3}$/.test(currency)) return String(value);
		return new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(value);
	}

	const average = $derived.by(() => {
		const l = number(low);
		const h = number(high);
		return l === null || h === null ? null : (l + h) / 2;
	});
</script>

<div class="amount">
	{#if !varies}
		<label>
			{label}
			<input bind:value={amount} required inputmode="decimal" autocomplete="off" placeholder="9.99" />
		</label>
	{:else}
		<fieldset>
			<legend>{label}</legend>
			<div class="row">
				<label class="grow">
					At least
					<input bind:value={low} inputmode="decimal" autocomplete="off" placeholder="optional" />
				</label>
				<label class="grow">
					At most
					<input bind:value={high} inputmode="decimal" autocomplete="off" placeholder="optional" />
				</label>
			</div>

			{#if !hasLow && !hasHigh}
				<p class="hint">Enter a lower bound, an upper bound, or both.</p>
			{:else if byHand}
				<label>
					Expected per payment
					<input bind:value={amount} required inputmode="decimal" autocomplete="off" />
				</label>
				<button type="button" class="link" onclick={() => (byHand = false)}>
					{hasLow && hasHigh ? 'Use the average instead' : 'Use the bound instead'}
				</button>
			{:else}
				<p class="hint">
					{#if hasLow && hasHigh}
						Expected: the average of the two, {money(average)}.
					{:else}
						Expected: the {hasHigh ? 'upper' : 'lower'} bound, {money(number(hasHigh ? high : low))}.
					{/if}
					<button type="button" class="link" onclick={() => (byHand = true)}>Enter it yourself</button>
				</p>
			{/if}
		</fieldset>
	{/if}

	<label class="check">
		<input type="checkbox" bind:checked={varies} />
		<span>The amount varies</span>
	</label>
</div>

<style>
	.amount {
		display: grid;
		gap: 0.5rem;
	}

	label,
	fieldset {
		display: grid;
		gap: 0.375rem;
		font-weight: 500;
	}

	fieldset {
		border: 0;
		margin: 0;
		padding: 0;
		gap: 0.5rem;
	}

	legend {
		padding: 0;
		margin-bottom: 0.375rem;
	}

	.row {
		display: flex;
		gap: 0.75rem;
	}

	.grow {
		flex: 1;
		min-width: 0;
	}

	.hint {
		margin: 0;
		color: var(--muted);
		font-size: 0.875rem;
		font-weight: 400;
	}

	.hint .link {
		min-height: 0;
		padding: 0;
		font-size: inherit;
	}

	button.link {
		justify-self: start;
	}

	.check {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: 400;
		cursor: pointer;
	}

	.check input {
		width: 1.25rem;
		height: 1.25rem;
		min-height: 0;
		margin: 0;
		accent-color: var(--accent);
	}
</style>
