<script lang="ts">
	import { onMount } from 'svelte';
	import { deleteSubscription, getOverview } from '#lib/api.ts';
	import type { Overview } from '#lib/bindings/Overview.ts';
	import type { SubscriptionView } from '#lib/bindings/SubscriptionView.ts';
	import AddSubscription from '#lib/components/AddSubscription.svelte';
	import {
		daysBetween,
		describeBilling,
		formatDate,
		formatMoney,
		isMonthly,
		relativeDays
	} from '#lib/format.ts';

	// `undefined` means the device's locale.
	const locale = undefined;

	let overview = $state<Overview | null>(null);
	let error = $state<string | null>(null);
	let adding = $state(false);
	// The subscription whose delete button was tapped once and awaits confirmation.
	let confirmingDelete = $state<string | null>(null);

	const statusLabels: Record<SubscriptionView['status'], string> = {
		active: 'Active',
		paused: 'Paused',
		cancelled: 'Cancelled',
		not_started: 'Not started'
	};

	onMount(async () => {
		try {
			overview = await getOverview();
		} catch (e) {
			error = String(e);
		}
	});

	async function remove(id: string) {
		try {
			overview = await deleteSubscription(id);
			confirmingDelete = null;
		} catch (e) {
			error = String(e);
		}
	}
</script>

<main>
	<header>
		<h1>Epicycle</h1>
	</header>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if overview}
		<section class="summary" aria-label="Recurring commitment">
			<div class="stat">
				<h2>Per month</h2>
				{#each overview.monthlyTotal as total (total.currency)}
					<p class="value">{formatMoney(total, locale)}</p>
				{:else}
					<p class="value">–</p>
				{/each}
			</div>
			<div class="stat">
				<h2>Per year</h2>
				{#each overview.annualTotal as total (total.currency)}
					<p class="value">{formatMoney(total, locale)}</p>
				{:else}
					<p class="value">–</p>
				{/each}
			</div>
			<p class="note">
				What active subscriptions cost at current prices, averaged per month and per year. Totals in
				different currencies are shown separately.
			</p>
		</section>

		{#if adding}
			<AddSubscription
				today={overview.today}
				onAdded={(updated) => {
					overview = updated;
					adding = false;
				}}
				onCancel={() => (adding = false)}
			/>
		{:else}
			<button class="add" onclick={() => (adding = true)}>Add subscription</button>
		{/if}

		<section aria-labelledby="list-heading">
			<h2 id="list-heading">Subscriptions</h2>
			{#if overview.subscriptions.length === 0}
				<p class="muted">No subscriptions yet. Add one to see what it costs per month.</p>
			{:else}
				<ul>
					{#each overview.subscriptions as s (s.id)}
						<li>
							<div class="main">
								<h3>{s.name}</h3>
								<p class="muted">
									{formatMoney(s.price, locale)} · {describeBilling(s.billing, s.nextPayment, locale)}
								</p>
								{#if s.monthly && !isMonthly(s.billing)}
									<p class="muted">≈ {formatMoney(s.monthly, locale)} per month</p>
								{/if}
								{#if s.status !== 'active'}
									<p class="badge">{statusLabels[s.status]}</p>
								{/if}
							</div>
							<div class="side">
								{#if s.nextPayment}
									<p>
										<span class="muted">Next</span>
										<time datetime={s.nextPayment}>{formatDate(s.nextPayment, locale)}</time>
									</p>
									<p class="muted">
										{relativeDays(daysBetween(overview.today, s.nextPayment), locale)}
									</p>
								{:else}
									<p class="muted">No upcoming payment</p>
								{/if}
								{#if confirmingDelete === s.id}
									<div class="confirm">
										<button class="secondary" onclick={() => (confirmingDelete = null)}>Keep</button>
										<button class="danger" onclick={() => remove(s.id)}>Delete</button>
									</div>
								{:else}
									<button
										class="link"
										onclick={() => (confirmingDelete = s.id)}
										aria-label="Delete {s.name}"
									>
										Delete
									</button>
								{/if}
							</div>
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

	.summary {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.75rem;
	}

	.stat {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 0.875rem 1rem;
	}

	.stat h2 {
		margin-bottom: 0.25rem;
		font-size: 0.875rem;
	}

	.value {
		font-size: 1.5rem;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
	}

	.note {
		grid-column: 1 / -1;
		color: var(--muted);
		font-size: 0.875rem;
	}

	.add {
		justify-self: start;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.75rem;
	}

	li {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 0.875rem 1rem;
	}

	.main {
		display: grid;
		gap: 0.25rem;
		align-content: start;
		min-width: 0;
	}

	.main h3 {
		overflow-wrap: anywhere;
	}

	.side {
		display: grid;
		gap: 0.25rem;
		justify-items: end;
		text-align: right;
		align-content: start;
		flex-shrink: 0;
	}

	.badge {
		justify-self: start;
		font-size: 0.75rem;
		font-weight: 600;
		padding: 0.125rem 0.5rem;
		border-radius: 999px;
		background: var(--border);
	}

	.confirm {
		display: flex;
		gap: 0.5rem;
	}

	time {
		font-variant-numeric: tabular-nums;
	}

	@media (max-width: 30rem) {
		li {
			flex-direction: column;
			gap: 0.5rem;
		}

		.side {
			justify-items: start;
			text-align: left;
		}
	}
</style>
