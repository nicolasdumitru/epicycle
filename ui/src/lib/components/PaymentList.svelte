<script lang="ts">
	// A subscription's payments around today, each changeable on its own: what
	// was actually paid, when, or (for a payment of 0) removing it. Changes are
	// recorded in the history like any other edit, and can be undone.
	import type { Edit } from '#lib/bindings/Edit.ts';
	import type { PaymentView } from '#lib/bindings/PaymentView.ts';
	import { daysBetween, formatDate, formatMoney, relativeDays } from '#lib/format.ts';
	import DateInput from './DateInput.svelte';

	let {
		payments,
		today,
		busy,
		onEdit
	}: {
		payments: PaymentView[];
		today: string;
		busy: boolean;
		/** Applies an edit; resolves to true if it succeeded. */
		onEdit: (edit: Edit) => Promise<boolean>;
	} = $props();

	// `undefined` means the device's locale.
	const locale = undefined;

	// The payment being changed, and the form's values.
	let editing = $state<{ payment: PaymentView; amount: string; date: string; removed: boolean } | null>(
		null
	);

	// An empty amount counts as nothing paid, so the remove option appears as
	// soon as the amount is cleared.
	const isZero = $derived(
		editing !== null &&
			(editing.amount.trim() === '' || Number(editing.amount.replace(',', '.')) === 0)
	);
	// Removing means nothing was paid: the amount is 0 without typing it.
	const removing = $derived(editing !== null && editing.removed && isZero);

	function open(payment: PaymentView) {
		editing = { payment, amount: payment.amount.amount, date: payment.date, removed: payment.removed };
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!editing) return;
		const { payment, amount, date, removed } = editing;
		// On an unchanged payment, record only what the user changed, so a later
		// price change still applies to the rest. A changed one is replaced
		// whole, so keep its values.
		const keepAll = payment.adjusted;
		const done = await onEdit(
			removing
				? { kind: 'payment', billed: payment.billed, amount: '0', date: '', removed: true }
				: {
						kind: 'payment',
						billed: payment.billed,
						amount: keepAll || amount !== payment.amount.amount ? amount : '',
						date: keepAll || date !== payment.date ? date : '',
						removed: false
					}
		);
		if (done) editing = null;
	}
</script>

{#if payments.length === 0}
	<p class="muted">No payments from three months ago to six months ahead.</p>
{:else}
	<ul>
		{#each payments as payment (payment.billed)}
			<li class:past={payment.date < today}>
				<div class="row">
					<div class="when">
						<span class:struck={payment.removed}>{formatDate(payment.date, locale)}</span>
						<span class="muted">
							{payment.date === today
								? 'today'
								: relativeDays(daysBetween(today, payment.date), locale)}
							{#if payment.billed !== payment.date && !payment.removed}
								· billed {formatDate(payment.billed, locale)}
							{/if}
						</span>
					</div>
					<div class="what">
						<span class="amount" class:struck={payment.removed}>{formatMoney(payment.amount, locale)}</span>
						{#if payment.removed}
							<span class="badge">Removed</span>
						{:else if payment.adjusted}
							<span class="badge">Changed</span>
						{/if}
					</div>
				</div>

				{#if editing?.payment.billed === payment.billed}
					<form class="change" onsubmit={save}>
						{#if !removing}
							<label>
								Amount ({payment.amount.currency})
								<input bind:value={editing.amount} required inputmode="decimal" autocomplete="off" />
							</label>
							<DateInput
								bind:value={editing.date}
								label={editing.date < today ? 'Paid on' : 'Due on'}
								required
								{today}
							/>
						{/if}
						{#if isZero}
							<label class="check">
								<input type="checkbox" bind:checked={editing.removed} />
								<span>Remove this payment (otherwise a payment of 0 keeps its date)</span>
							</label>
						{/if}
						{#if removing}
							<p class="muted">Nothing paid: this payment won't appear or count anywhere.</p>
						{/if}
						<div class="actions">
							<button type="button" class="secondary" onclick={() => (editing = null)}>Cancel</button>
							<button type="submit" disabled={busy}>Save</button>
						</div>
					</form>
				{:else}
					<div class="buttons">
						<button class="link" onclick={() => open(payment)} disabled={busy}>Change…</button>
						{#if payment.adjusted}
							<button
								class="link"
								onclick={() => onEdit({ kind: 'reset_payment', billed: payment.billed })}
								disabled={busy}>Undo change</button
							>
						{/if}
					</div>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
	}

	li {
		display: grid;
		gap: 0.25rem;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--border);
	}

	li:last-child {
		border-bottom: 0;
	}

	.past .when > span:first-child {
		color: var(--muted);
	}

	.row {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
	}

	.when {
		display: grid;
		min-width: 0;
	}

	.muted {
		color: var(--muted);
		font-size: 0.875rem;
	}

	p.muted {
		margin: 0;
	}

	.what {
		display: grid;
		justify-items: end;
		align-content: start;
		gap: 0.125rem;
	}

	.amount {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.struck {
		text-decoration: line-through;
		color: var(--muted);
	}

	.badge {
		font-size: 0.75rem;
		font-weight: 600;
		padding: 0.0625rem 0.5rem;
		border-radius: 999px;
		background: var(--border);
	}

	.buttons {
		display: flex;
		gap: 0.75rem;
	}

	.buttons .link {
		min-height: 36px;
		padding: 0;
	}

	.change {
		display: grid;
		gap: 0.75rem;
		padding: 0.5rem 0;
	}

	label {
		display: grid;
		gap: 0.375rem;
		font-weight: 500;
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
		flex: none;
		accent-color: var(--accent);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.75rem;
	}
</style>
