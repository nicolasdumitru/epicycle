<script lang="ts">
	// Payments due soon, grouped by day, each in its own currency.
	import { getUpcoming } from '#lib/api.ts';
	import type { UpcomingPayment } from '#lib/bindings/UpcomingPayment.ts';
	import { daysBetween, formatDate, formatMoney, relativeDays } from '#lib/format.ts';

	let { today, reloadKey }: { today: string; reloadKey: unknown } = $props();

	// `undefined` means the device's locale.
	const locale = undefined;

	let days = $state(30);
	let payments = $state<UpcomingPayment[] | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		void reloadKey;
		const asked = days;
		getUpcoming(asked)
			.then((p) => {
				if (asked === days) payments = p;
			})
			.catch((e) => (error = String(e)));
	});

	// Consecutive payments on the same date, in date order.
	const byDate = $derived.by(() => {
		const groups: { date: string; payments: UpcomingPayment[] }[] = [];
		for (const p of payments ?? []) {
			const last = groups.at(-1);
			if (last?.date === p.date) last.payments.push(p);
			else groups.push({ date: p.date, payments: [p] });
		}
		return groups;
	});
</script>

<section aria-labelledby="upcoming-heading">
	<h2 id="upcoming-heading">Upcoming payments</h2>
	{#if error}
		<p class="error" role="alert">{error}</p>
	{:else if payments}
		{#if byDate.length === 0}
			<p class="muted">Nothing due in the next {days} days.</p>
		{:else}
			<ol class="card">
				{#each byDate as group (group.date)}
					<li>
						<p class="when">
							<time datetime={group.date}>{formatDate(group.date, locale)}</time>
							<span class="muted">· {relativeDays(daysBetween(today, group.date), locale)}</span>
						</p>
						<ul>
							{#each group.payments as p (p.subscriptionId)}
								<li>
									<span class="what">
										<a href="/subscriptions/{p.subscriptionId}">{p.name}</a>
										{#if p.billed !== p.date}
											<span class="billed">billed {formatDate(p.billed, locale)}</span>
										{/if}
									</span>
									<span class="amount">{formatMoney(p.amount, locale)}</span>
								</li>
							{/each}
						</ul>
					</li>
				{/each}
			</ol>
		{/if}
		<button class="link" onclick={() => (days = days === 30 ? 90 : 30)}>
			{days === 30 ? 'Show the next 90 days' : 'Show the next 30 days'}
		</button>
	{:else}
		<p class="muted">Loading…</p>
	{/if}
</section>

<style>
	section {
		display: grid;
		gap: 0.5rem;
	}

	h2 {
		margin: 0;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
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
		list-style: none;
		margin: 0;
		display: grid;
		gap: 0.75rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 0.875rem 1rem;
	}

	.when {
		font-size: 0.875rem;
		font-weight: 600;
	}

	ul {
		list-style: none;
		margin: 0.25rem 0 0;
		padding: 0;
		display: grid;
	}

	ul li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		min-height: 44px;
	}

	a {
		color: inherit;
		text-decoration: none;
		overflow-wrap: anywhere;
	}

	a:hover {
		text-decoration: underline;
	}

	.what {
		display: grid;
		min-width: 0;
	}

	.billed {
		color: var(--muted);
		font-size: 0.8125rem;
	}

	.amount {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	button.link {
		justify-self: start;
		min-height: 44px;
	}
</style>
