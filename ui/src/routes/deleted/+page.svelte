<script lang="ts">
	// Deleted subscriptions, which keep their history and can be restored.
	import { goto } from '$app/navigation';
	import { getDeleted, restoreRevision } from '#lib/api.ts';
	import type { DeletedView } from '#lib/bindings/DeletedView.ts';
	import { describeBilling, formatMoney, formatTimestamp } from '#lib/format.ts';

	// `undefined` means the device's locale.
	const locale = undefined;

	let deleted = $state<DeletedView[] | null>(null);
	let error = $state<string | null>(null);
	let restoring = $state<string | null>(null);

	$effect(() => {
		getDeleted()
			.then((d) => (deleted = d))
			.catch((e) => (error = String(e)));
	});

	async function restore(item: DeletedView) {
		restoring = item.id;
		error = null;
		try {
			await restoreRevision(item.id, item.revisionId);
			await goto(`/subscriptions/${item.id}`);
		} catch (e) {
			error = String(e);
			restoring = null;
		}
	}
</script>

<main>
	<nav>
		<a href="/subscriptions" class="back">‹ Subscriptions</a>
	</nav>
	<h1>Deleted subscriptions</h1>
	<p class="muted">
		Deleted subscriptions keep their history. Restoring one brings back the version that was
		deleted.
	</p>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#if deleted}
		{#if deleted.length === 0}
			<p class="muted">Nothing has been deleted.</p>
		{:else}
			<ul>
				{#each deleted as item (item.id)}
					<li>
						<div>
							<h2>{item.name}</h2>
							<p class="muted">
								{formatMoney(item.price, locale)} · {describeBilling(item.billing, null, locale)}
							</p>
							<p class="muted">Deleted {formatTimestamp(item.deletedAt, locale)}</p>
						</div>
						<button
							class="secondary"
							onclick={() => restore(item)}
							disabled={restoring !== null}
						>
							{restoring === item.id ? 'Restoring…' : 'Restore'}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
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
		gap: 1rem;
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
		margin: 0;
		font-size: 1rem;
		overflow-wrap: anywhere;
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
		align-items: center;
		gap: 1rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 0.875rem 1rem;
	}

	li > div {
		display: grid;
		gap: 0.25rem;
		min-width: 0;
	}
</style>
