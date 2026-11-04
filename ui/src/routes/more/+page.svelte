<script lang="ts">
	// The More tab: backups and exchange rates.
	import BackupPanel from '#lib/components/BackupPanel.svelte';
</script>

<main>
	<header>
		<h1>More</h1>
	</header>

	<BackupPanel onImported={() => {}} />

	<section aria-labelledby="reminders-heading">
		<h2 id="reminders-heading">Reminders</h2>
		<p class="muted">Notifications before payments, trials ending and price changes.</p>
		<a class="link" href="/reminders">Reminders ›</a>
	</section>

	<section aria-labelledby="rates-heading">
		<h2 id="rates-heading">Exchange rates</h2>
		<p class="muted">The rates used to show totals in one currency, and your own.</p>
		<a class="link" href="/rates">Exchange rates ›</a>
	</section>
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

	/* Grid items default to their content's minimum width, which would let a
	   wide card (or the chart) push the column past a phone's screen. */
	main > :global(*) {
		min-width: 0;
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

	h2 {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
	}

	section {
		display: grid;
		gap: 0.25rem;
	}

	.link {
		display: inline-flex;
		align-items: center;
		min-height: 44px;
		color: var(--accent);
		font-weight: 500;
		text-decoration: none;
	}
</style>
