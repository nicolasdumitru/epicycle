<script lang="ts">
	// One subscription: its details and notes, price and billing history, and
	// status. Every change here records a revision in the subscription's history.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { deleteSubscription, editSubscription, getSubscription } from '#lib/api.ts';
	import type { Edit } from '#lib/bindings/Edit.ts';
	import type { StatusChoice } from '#lib/bindings/StatusChoice.ts';
	import type { SubscriptionDetail } from '#lib/bindings/SubscriptionDetail.ts';
	import type { Unit } from '#lib/bindings/Unit.ts';
	import type { EstimateInput } from '#lib/bindings/EstimateInput.ts';
	import AmountInput from '#lib/components/AmountInput.svelte';
	import type { DueChoice } from '#lib/bindings/DueChoice.ts';
	import DateInput from '#lib/components/DateInput.svelte';
	import DueInput from '#lib/components/DueInput.svelte';
	import HistoryList from '#lib/components/HistoryList.svelte';
	import PaymentList from '#lib/components/PaymentList.svelte';
	import {
		billingUnits,
		describeBilling,
		describeDue,
		describePrice,
		isFree,
		formatDate,
		formatMoney,
		formatPrice,
		statusChoiceLabels
	} from '#lib/format.ts';

	// `undefined` means the device's locale.
	const locale = undefined;
	const id = $derived(page.params.id ?? '');

	let detail = $state<SubscriptionDetail | null>(null);
	let error = $state<string | null>(null);
	let busy = $state(false);

	// Details form.
	let name = $state('');
	let notes = $state('');

	// Plan form; null while closed.
	let planForm = $state<{
		from: string;
		amount: string;
		estimate: EstimateInput | null;
		due: DueChoice;
		every: number;
		unit: Unit;
		nextPayment: string;
	} | null>(null);

	// Status form; null while closed.
	let statusForm = $state<{ status: StatusChoice; from: string } | null>(null);

	let confirmingDelete = $state(false);

	const sub = $derived(detail?.subscription);
	const detailsChanged = $derived(
		// Mirrors how Rust normalises them, so whitespace alone isn't a change.
		sub !== undefined && (name.trim() !== sub.name || notes.trimEnd() !== sub.notes)
	);

	function show(d: SubscriptionDetail) {
		detail = d;
		name = d.subscription.name;
		notes = d.subscription.notes;
	}

	$effect(() => {
		const current = id;
		getSubscription(current)
			.then((d) => {
				if (current === id) show(d);
			})
			.catch((e) => (error = String(e)));
	});

	/** Applies an edit; true if it succeeded (errors show at the top). */
	async function apply(edit: Edit): Promise<boolean> {
		busy = true;
		error = null;
		try {
			show(await editSubscription(id, edit));
			planForm = null;
			statusForm = null;
			return true;
		} catch (e) {
			error = String(e);
			window.scrollTo({ top: 0, behavior: 'smooth' });
			return false;
		} finally {
			busy = false;
		}
	}

	function saveDetails(event: SubmitEvent) {
		event.preventDefault();
		apply({ kind: 'details', name, notes });
	}

	function openPlanForm() {
		if (!detail || !sub) return;
		statusForm = null;
		planForm = {
			from: detail.today,
			amount: sub.price.amount,
			due: sub.due,
			estimate: sub.estimate
				? {
						low: sub.estimate.low?.amount ?? '',
						high: sub.estimate.high?.amount ?? '',
						expected: sub.estimate.expected
					}
				: null,
			every: sub.billing.every,
			unit: sub.billing.unit,
			nextPayment: sub.nextPayment ?? detail.today
		};
	}

	function savePlan(event: SubmitEvent) {
		event.preventDefault();
		if (planForm) apply({ kind: 'plan', ...planForm });
	}

	function openStatusForm(status: StatusChoice) {
		if (!detail) return;
		planForm = null;
		statusForm = { status, from: detail.today };
	}

	function saveStatus(event: SubmitEvent) {
		event.preventDefault();
		if (statusForm) apply({ kind: 'status', ...statusForm });
	}

	async function remove() {
		busy = true;
		try {
			await deleteSubscription(id);
			await goto('/subscriptions');
		} catch (e) {
			error = String(e);
			busy = false;
		}
	}

	const statusVerbs: Record<StatusChoice, string> = {
		active: 'Resume',
		paused: 'Pause',
		cancelled: 'Cancel subscription'
	};

	// The current status as a choice; "not started" counts as active.
	const currentStatus = $derived<StatusChoice>(
		sub?.status === 'paused' || sub?.status === 'cancelled' ? sub.status : 'active'
	);
</script>

<main>
	<nav>
		<a href="/subscriptions" class="back">‹ Subscriptions</a>
	</nav>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if detail && sub}
		<header>
			<h1>{sub.name}</h1>
			<p class="muted">
				{describePrice(sub, locale)}
			</p>
			<p class="muted">
				{#if sub.nextPayment}
					Next payment {formatDate(sub.nextPayment, locale)}
				{:else}
					No upcoming payment
				{/if}
				· {statusChoiceLabels[currentStatus]}
			</p>
		</header>

		<section aria-labelledby="details-heading">
			<h2 id="details-heading">Details</h2>
			<form class="card" onsubmit={saveDetails}>
				<label>
					Name
					<input bind:value={name} required autocomplete="off" />
				</label>
				<label>
					Notes
					<textarea bind:value={notes} rows="5" placeholder="Anything worth remembering: who shares it, how to cancel, …"
					></textarea>
				</label>
				<div class="actions">
					{#if detailsChanged}
						<button
							type="button"
							class="secondary"
							onclick={() => {
								name = sub.name;
								notes = sub.notes;
							}}>Discard</button
						>
					{/if}
					<button type="submit" disabled={busy || !detailsChanged}>Save</button>
				</div>
			</form>
		</section>

		<section aria-labelledby="plan-heading">
			<h2 id="plan-heading">Price and billing</h2>
			<div class="card">
				<ol class="timeline">
					{#each detail.plans.toReversed() as plan (plan.from)}
						<li>
							<span class="when">From {formatDate(plan.from, locale)}</span>
							<span>
								{isFree(plan.price)
									? 'Free: no payments'
									: `${formatPrice(plan.price, plan.estimate, locale)} · ${describeBilling(plan.billing, plan.anchor, locale)}${describeDue(plan.due)}`}
							</span>
						</li>
					{/each}
				</ol>

				{#if planForm}
					<form class="subform" onsubmit={savePlan}>
						<h3>Change price or billing</h3>
						<DateInput
							bind:value={planForm.from}
							label="Takes effect on"
							hint="Payments before this date keep the old price."
							required
							today={detail.today}
						/>
						<AmountInput
							bind:amount={planForm.amount}
							bind:estimate={planForm.estimate}
							currency={sub.price.currency}
							label="Price per payment ({sub.price.currency})"
						/>
						<fieldset>
							<legend>Billed every</legend>
							<div class="row">
								<input
									class="every"
									type="number"
									min="1"
									max="65535"
									bind:value={planForm.every}
									required
									aria-label="Interval"
								/>
								<select bind:value={planForm.unit} aria-label="Unit">
									{#each billingUnits as u (u.value)}
										<option value={u.value}>{planForm.every === 1 ? u.one : u.many}</option>
									{/each}
								</select>
							</div>
						</fieldset>
						<DueInput bind:due={planForm.due} />
						<DateInput
							bind:value={planForm.nextPayment}
							label={planForm.due.kind === 'on_billing'
								? 'First payment at the new price'
								: 'First bill at the new price'}
							hint="Nothing is paid before it. Monthly and yearly billing repeat on this day."
							required
							today={detail.today}
						/>
						<div class="actions">
							<button type="button" class="secondary" onclick={() => (planForm = null)}>Cancel</button>
							<button type="submit" disabled={busy}>Save change</button>
						</div>
					</form>
				{:else}
					<div class="actions start">
						<button class="secondary" onclick={openPlanForm}>Change price or billing…</button>
					</div>
				{/if}
			</div>
		</section>

		<section aria-labelledby="payments-heading">
			<h2 id="payments-heading">Payments</h2>
			<div class="card">
				<p class="muted">
					Change one payment when it was different: another amount, another day, or skipped.
				</p>
				<PaymentList payments={detail.payments} today={detail.today} {busy} onEdit={apply} />
			</div>
		</section>

		<section aria-labelledby="status-heading">
			<h2 id="status-heading">Status</h2>
			<div class="card">
				<ol class="timeline">
					{#each detail.statusChanges.toReversed() as change (change.from)}
						<li>
							<span class="when">From {formatDate(change.from, locale)}</span>
							<span>{statusChoiceLabels[change.status]}</span>
						</li>
					{/each}
					<li>
						<span class="when">From {formatDate(sub.start, locale)}</span>
						<span>Started</span>
					</li>
				</ol>

				{#if statusForm}
					<form class="subform" onsubmit={saveStatus}>
						<h3>{statusVerbs[statusForm.status]}</h3>
						<DateInput
							bind:value={statusForm.from}
							label="From"
							hint={statusForm.status === 'active'
								? 'Payments resume on the usual schedule from this date.'
								: 'No payments are counted from this date on.'}
							required
							today={detail.today}
						/>
						<div class="actions">
							<button type="button" class="secondary" onclick={() => (statusForm = null)}>Back</button>
							<button type="submit" disabled={busy}>{statusVerbs[statusForm.status]}</button>
						</div>
					</form>
				{:else}
					<div class="actions start">
						{#if currentStatus === 'active'}
							<button class="secondary" onclick={() => openStatusForm('paused')}>Pause…</button>
							<button class="secondary" onclick={() => openStatusForm('cancelled')}>Cancel subscription…</button>
						{:else}
							<button class="secondary" onclick={() => openStatusForm('active')}>Resume…</button>
							{#if currentStatus === 'paused'}
								<button class="secondary" onclick={() => openStatusForm('cancelled')}
									>Cancel subscription…</button
								>
							{/if}
						{/if}
					</div>
				{/if}
			</div>
		</section>

		<section aria-labelledby="history-heading">
			<h2 id="history-heading">History</h2>
			<div class="card">
				<p class="muted">
					Every change is kept. Restoring an earlier version adds it as a new change, so you can
					always go back again.
				</p>
				<HistoryList {id} reloadKey={detail} onRestored={show} />
			</div>
		</section>

		<section aria-labelledby="delete-heading">
			<h2 id="delete-heading">Delete</h2>
			<div class="card">
				<p class="muted">
					Removes it from the list and from all totals. Its history is kept and goes into backups. To
					stop counting future payments but keep the past ones, cancel it instead.
				</p>
				<div class="actions start">
					{#if confirmingDelete}
						<button class="secondary" onclick={() => (confirmingDelete = false)}>Keep</button>
						<button class="danger" onclick={remove} disabled={busy}>Delete {sub.name}</button>
					{:else}
						<button class="secondary" onclick={() => (confirmingDelete = true)}>Delete…</button>
					{/if}
				</div>
			</div>
		</section>
	{:else if !error}
		<p class="muted">Loading…</p>
	{/if}
</main>

<style>
	main {
		max-width: 44rem;
		margin: 0 auto;
		/* 16px from the window's edges on phones, growing to 32px on wider windows. */
		padding: 1rem max(1rem, min(4vw, 2rem));
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

	header {
		display: grid;
		gap: 0.25rem;
	}

	h1 {
		margin: 0;
		font-size: 1.5rem;
		overflow-wrap: anywhere;
	}

	h2 {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
	}

	h3 {
		margin: 0;
		font-size: 1rem;
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

	.card {
		display: grid;
		gap: 1rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	.subform {
		display: grid;
		gap: 1rem;
		border-top: 1px solid var(--border);
		padding-top: 1rem;
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

	textarea {
		font: inherit;
		font-size: max(1rem, 16px);
		box-sizing: border-box;
		width: 100%;
		padding: 0.5rem 0.75rem;
		color: var(--fg);
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 0.5rem;
		resize: vertical;
		font-weight: 400;
	}

	.row {
		display: flex;
		gap: 0.75rem;
	}

	.every {
		width: 5rem;
	}

	select {
		flex: 1;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 0.75rem;
	}

	.actions.start {
		justify-content: flex-start;
	}

	.timeline {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.5rem;
	}

	.timeline li {
		display: grid;
		gap: 0.125rem;
	}

	.when {
		color: var(--muted);
		font-size: 0.875rem;
	}
</style>
