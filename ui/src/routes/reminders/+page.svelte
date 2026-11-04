<script lang="ts">
	// Reminder settings: whether to remind, how many days ahead (one or more),
	// and at what time of day. Reminders cover payments due, bills on a
	// separate day, free trials ending and price changes (docs/DECISIONS.md,
	// "Reminders"). Scheduled on Android; desktop can't schedule yet.
	import { onMount } from 'svelte';
	import {
		getNotificationAccess,
		getReminderSettings,
		requestNotificationAccess,
		setReminderSettings
	} from '#lib/api.ts';
	import type { NotificationAccess } from '#lib/bindings/NotificationAccess.ts';

	let enabled = $state(true);
	let leadDays = $state<number[]>([]);
	let time = $state('09:00');
	let access = $state<NotificationAccess | null>(null);
	let loaded = $state(false);
	let custom = $state('');
	let saving = $state(false);
	let message = $state<string | null>(null);
	let error = $state<string | null>(null);

	const presets = [0, 1, 3, 7];
	const MAX = 5;

	onMount(async () => {
		try {
			const [settings, a] = await Promise.all([getReminderSettings(), getNotificationAccess()]);
			enabled = settings.enabled;
			leadDays = settings.leadDays;
			time = settings.timeOfDay;
			access = a;
			loaded = true;
		} catch (e) {
			error = String(e);
		}
	});

	function label(days: number): string {
		if (days === 0) return 'On the day';
		return days === 1 ? '1 day before' : `${days} days before`;
	}

	function add(days: number) {
		if (!Number.isInteger(days) || days < 0 || days > 365) {
			error = 'Choose between 0 and 365 days before.';
			return;
		}
		error = null;
		if (!leadDays.includes(days)) leadDays = [...leadDays, days].sort((a, b) => a - b);
	}

	function remove(days: number) {
		leadDays = leadDays.filter((d) => d !== days);
	}

	async function allow() {
		error = null;
		try {
			access = await requestNotificationAccess();
		} catch (e) {
			error = String(e);
		}
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		saving = true;
		error = null;
		message = null;
		try {
			const saved = await setReminderSettings({ enabled, leadDays, timeOfDay: time });
			leadDays = saved.leadDays;
			time = saved.timeOfDay;
			message = 'Saved.';
		} catch (e) {
			error = String(e);
		} finally {
			saving = false;
		}
	}
</script>

<main>
	<nav>
		<a href="/more" class="back">‹ More</a>
	</nav>
	<h1>Reminders</h1>
	<p class="muted">
		A notification before each payment is due, and when a bill comes on another day, a free trial
		ends or a new price starts.
	</p>

	{#if access === 'unsupported'}
		<p class="note">
			Reminders are sent on Android for now. These settings are kept, and travel in backups.
		</p>
	{:else if access === 'denied'}
		<p class="warning">
			Notifications are turned off for Epicycle. Turn them on in your phone's settings, under
			Apps › Epicycle › Notifications.
		</p>
	{:else if access === 'prompt'}
		<div class="card allow">
			<p>Epicycle needs your permission to show notifications.</p>
			<button onclick={allow}>Allow notifications</button>
		</div>
	{/if}

	{#if loaded}
		<form class="card" onsubmit={save}>
			<label class="check">
				<input type="checkbox" bind:checked={enabled} />
				<span>Remind me</span>
			</label>

			<fieldset disabled={!enabled}>
				<legend>When</legend>
				{#if leadDays.length === 0}
					<p class="muted">No reminders yet: add one below.</p>
				{:else}
					<ul class="chips">
						{#each leadDays as days (days)}
							<li>
								{label(days)}
								<button
									type="button"
									class="remove"
									aria-label="Remove {label(days)}"
									onclick={() => remove(days)}>×</button
								>
							</li>
						{/each}
					</ul>
				{/if}

				{#if leadDays.length < MAX}
					<div class="add">
						{#each presets.filter((d) => !leadDays.includes(d)) as days (days)}
							<button type="button" class="secondary small" onclick={() => add(days)}>
								+ {label(days)}
							</button>
						{/each}
					</div>
					<div class="row">
						<input
							class="days"
							type="number"
							min="0"
							max="365"
							bind:value={custom}
							placeholder="Days"
							aria-label="Days before"
						/>
						<button
							type="button"
							class="secondary"
							disabled={custom === ''}
							onclick={() => {
								add(Number(custom));
								custom = '';
							}}>Add</button
						>
					</div>
				{:else}
					<p class="muted">Up to {MAX} reminders per event.</p>
				{/if}

				<label>
					At
					<input class="time" type="time" bind:value={time} required />
				</label>
			</fieldset>

			{#if error}
				<p class="error" role="alert">{error}</p>
			{/if}
			{#if message}
				<p role="status" class="muted">{message}</p>
			{/if}

			<div class="actions">
				<button type="submit" disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
			</div>
		</form>
	{:else if error}
		<p class="error" role="alert">{error}</p>
	{:else}
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
		gap: 1rem;
	}

	main > :global(*) {
		min-width: 0;
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

	p {
		margin: 0;
	}

	.muted {
		color: var(--muted);
	}

	.note {
		color: var(--muted);
		font-size: 0.875rem;
	}

	.warning,
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

	.allow {
		gap: 0.75rem;
		justify-items: start;
	}

	fieldset {
		border: 0;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.75rem;
	}

	fieldset:disabled {
		opacity: 0.5;
	}

	legend {
		padding: 0;
		margin-bottom: 0.5rem;
		font-weight: 500;
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
		cursor: pointer;
	}

	.check input {
		width: 1.25rem;
		height: 1.25rem;
		min-height: 0;
		margin: 0;
		accent-color: var(--accent);
	}

	.chips {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.chips li {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0 0.25rem 0 0.75rem;
		border: 1px solid var(--accent);
		border-radius: 999px;
		font-weight: 600;
	}

	.remove {
		min-height: 36px;
		width: 36px;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--muted);
		font-size: 1.25rem;
	}

	.add {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.small {
		min-height: 36px;
		padding: 0 0.75rem;
		font-size: 0.875rem;
	}

	.row {
		display: flex;
		gap: 0.5rem;
	}

	.days {
		width: 7rem;
	}

	.time {
		width: 9rem;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
	}
</style>
