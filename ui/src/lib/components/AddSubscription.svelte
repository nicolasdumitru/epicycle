<script lang="ts">
	import { addSubscription } from '#lib/api.ts';
	import type { Overview } from '#lib/bindings/Overview.ts';
	import type { Unit } from '#lib/bindings/Unit.ts';

	let {
		today,
		onAdded,
		onCancel
	}: { today: string; onAdded: (overview: Overview) => void; onCancel: () => void } = $props();

	// Suggestions only; any ISO 4217 code is accepted and validated in Rust.
	const commonCurrencies = ['EUR', 'USD', 'GBP', 'RON', 'CHF', 'PLN', 'CZK', 'HUF', 'SEK', 'NOK', 'DKK', 'JPY'];

	const units: { value: Unit; one: string; many: string }[] = [
		{ value: 'days', one: 'day', many: 'days' },
		{ value: 'weeks', one: 'week', many: 'weeks' },
		{ value: 'months', one: 'month', many: 'months' },
		{ value: 'years', one: 'year', many: 'years' }
	];

	let name = $state('');
	let amount = $state('');
	let currency = $state('EUR');
	let every = $state(1);
	let unit = $state<Unit>('months');
	// svelte-ignore state_referenced_locally (only the initial value is wanted)
	let nextPayment = $state(today);
	let since = $state('');
	let error = $state<string | null>(null);
	let saving = $state(false);
	let nameInput: HTMLInputElement | undefined = $state();

	// The form opens because the user asked to add something: start typing right away.
	$effect(() => nameInput?.focus());

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = null;
		try {
			const overview = await addSubscription({
				name,
				amount,
				currency,
				every,
				unit,
				nextPayment,
				since: since === '' ? null : since
			});
			onAdded(overview);
		} catch (e) {
			error = String(e);
		} finally {
			saving = false;
		}
	}
</script>

<form onsubmit={submit} aria-labelledby="add-heading">
	<h2 id="add-heading">Add subscription</h2>

	<label>
		Name
		<input
			bind:this={nameInput}
			bind:value={name}
			required
			autocomplete="off"
			placeholder="e.g. Music streaming"
		/>
	</label>

	<div class="row">
		<label class="grow">
			Price per payment
			<input bind:value={amount} required inputmode="decimal" autocomplete="off" placeholder="9.99" />
		</label>
		<label class="currency">
			Currency
			<input
				bind:value={currency}
				required
				list="currencies"
				maxlength="3"
				autocapitalize="characters"
				autocomplete="off"
			/>
			<datalist id="currencies">
				{#each commonCurrencies as code (code)}
					<option value={code}></option>
				{/each}
			</datalist>
		</label>
	</div>

	<fieldset>
		<legend>Billed every</legend>
		<div class="row">
			<input
				class="every"
				type="number"
				min="1"
				max="65535"
				bind:value={every}
				required
				aria-label="Interval"
			/>
			<select bind:value={unit} aria-label="Unit">
				{#each units as u (u.value)}
					<option value={u.value}>{every === 1 ? u.one : u.many}</option>
				{/each}
			</select>
		</div>
	</fieldset>

	<label>
		Next payment
		<input type="date" bind:value={nextPayment} required />
		<span class="hint">Monthly and yearly billing repeat on this day.</span>
	</label>

	<label>
		Subscribed since <span class="optional">(optional)</span>
		<input type="date" bind:value={since} />
		<span class="hint">Used to estimate past spending.</span>
	</label>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	<div class="actions">
		<button type="button" class="secondary" onclick={onCancel}>Cancel</button>
		<button type="submit" disabled={saving}>{saving ? 'Saving…' : 'Add'}</button>
	</div>
</form>

<style>
	form {
		display: grid;
		gap: 1rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	h2 {
		margin: 0;
		font-size: 1.125rem;
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
	}

	.currency {
		width: 6rem;
	}

	.every {
		width: 5rem;
	}

	select {
		flex: 1;
	}

	.hint,
	.optional {
		color: var(--muted);
		font-size: 0.875rem;
		font-weight: 400;
	}

	.error {
		margin: 0;
		color: var(--danger);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.75rem;
	}
</style>
