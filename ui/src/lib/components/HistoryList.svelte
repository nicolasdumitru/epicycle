<script lang="ts">
	// A subscription's change history, newest first, with a way to restore any
	// earlier version. Restoring adds a new entry; nothing is ever rewritten.
	import { getHistory, restoreRevision } from '#lib/api.ts';
	import type { RevisionView } from '#lib/bindings/RevisionView.ts';
	import type { SubscriptionDetail } from '#lib/bindings/SubscriptionDetail.ts';
	import { describeChange, formatTimestamp } from '#lib/format.ts';

	let {
		id,
		reloadKey,
		onRestored
	}: {
		id: string;
		/** Anything that changes when the subscription does, to reload the history. */
		reloadKey: unknown;
		onRestored: (detail: SubscriptionDetail) => void;
	} = $props();

	// `undefined` means the device's locale.
	const locale = undefined;
	const initiallyShown = 5;

	let revisions = $state<RevisionView[]>([]);
	let error = $state<string | null>(null);
	let showAll = $state(false);
	let restoring = $state<string | null>(null);

	const shown = $derived(showAll ? revisions : revisions.slice(0, initiallyShown));

	$effect(() => {
		void reloadKey;
		getHistory(id)
			.then((r) => (revisions = r))
			.catch((e) => (error = String(e)));
	});

	async function restore(revision: string) {
		restoring = revision;
		error = null;
		try {
			onRestored(await restoreRevision(id, revision));
		} catch (e) {
			error = String(e);
		} finally {
			restoring = null;
		}
	}
</script>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}

<ol>
	{#each shown as revision (revision.id)}
		<li>
			<p class="when">
				<time datetime={revision.createdAt}>{formatTimestamp(revision.createdAt, locale)}</time>
				{#if !revision.thisDevice}· from another device{/if}
				{#if revision.merged}· settled a conflict{/if}
				{#if revision.cause === 'imported' && !revision.merged}· imported{/if}
				{#if revision.current}<span class="badge">Current</span>{/if}
			</p>
			<ul>
				{#each revision.changes as change, i (i)}
					<li>{describeChange(change, locale)}</li>
				{:else}
					<li class="muted">No visible change</li>
				{/each}
			</ul>
			{#if revision.restorable}
				<button
					class="secondary small"
					onclick={() => restore(revision.id)}
					disabled={restoring !== null}
				>
					{restoring === revision.id ? 'Restoring…' : 'Restore this version'}
				</button>
			{/if}
		</li>
	{/each}
</ol>

{#if revisions.length > initiallyShown}
	<button class="link" onclick={() => (showAll = !showAll)}>
		{showAll ? 'Show fewer' : `Show all ${revisions.length} changes`}
	</button>
{/if}

<style>
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.875rem;
	}

	ol > li {
		display: grid;
		gap: 0.25rem;
		justify-items: start;
		padding-left: 0.75rem;
		border-left: 2px solid var(--border);
	}

	p {
		margin: 0;
	}

	.when {
		color: var(--muted);
		font-size: 0.875rem;
	}

	ul {
		margin: 0;
		padding-left: 1.125rem;
	}

	.muted {
		color: var(--muted);
	}

	.error {
		color: var(--danger);
	}

	.badge {
		margin-left: 0.375rem;
		font-size: 0.75rem;
		font-weight: 600;
		padding: 0.0625rem 0.5rem;
		border-radius: 999px;
		background: var(--border);
		color: var(--fg);
	}

	.small {
		margin-top: 0.25rem;
	}
</style>
