<script lang="ts">
	// When payments fall due after their bill: when billed (the default), some
	// days later, or on a day of the month (the first such day on or after the
	// bill). Upcoming payments and monthly cash flow use the due date.
	import type { DueChoice } from '#lib/bindings/DueChoice.ts';

	let { due = $bindable() }: { due: DueChoice } = $props();

	const id = $props.id();
	// The numbers are kept while switching between kinds.
	// svelte-ignore state_referenced_locally (the starting values only)
	let kind = $state<DueChoice['kind']>(due.kind);
	// svelte-ignore state_referenced_locally
	let days = $state(due.kind === 'days_after' ? due.days : 14);
	// svelte-ignore state_referenced_locally
	let day = $state(due.kind === 'day_of_month' ? due.day : 20);

	$effect(() => {
		due =
			kind === 'days_after'
				? { kind, days: Math.max(0, Math.trunc(days || 0)) }
				: kind === 'day_of_month'
					? { kind, day: Math.min(31, Math.max(1, Math.trunc(day || 1))) }
					: { kind: 'on_billing' };
	});
</script>

<div class="due">
	<label for="{id}-kind">Payment due</label>
	<div class="row">
		<select id="{id}-kind" bind:value={kind}>
			<option value="on_billing">When billed</option>
			<option value="days_after">Days after billing</option>
			<option value="day_of_month">On a set day</option>
		</select>
		{#if kind === 'days_after'}
			<input
				class="number"
				type="number"
				min="0"
				max="365"
				bind:value={days}
				required
				aria-label="Days after the bill"
			/>
		{:else if kind === 'day_of_month'}
			<input
				class="number"
				type="number"
				min="1"
				max="31"
				bind:value={day}
				required
				aria-label="Day of the month"
			/>
		{/if}
	</div>
	<span class="hint">
		{#if kind === 'days_after'}
			Due {days === 1 ? 'a day' : `${days} days`} after each bill.
		{:else if kind === 'day_of_month'}
			Due on day {day} (31 is the last day of the month), the first after each bill.
		{:else}
			Paid the day it's billed.
		{/if}
	</span>
</div>

<style>
	.due {
		display: grid;
		gap: 0.375rem;
	}

	label {
		font-weight: 500;
	}

	.row {
		display: flex;
		gap: 0.75rem;
	}

	select {
		flex: 1;
		min-width: 0;
	}

	.number {
		width: 5.5rem;
		flex: none;
	}

	.hint {
		color: var(--muted);
		font-size: 0.875rem;
	}
</style>
