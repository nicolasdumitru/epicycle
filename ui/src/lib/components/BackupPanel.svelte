<script lang="ts">
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { exportBackup, importBackup, previewImport } from '#lib/api.ts';
	import type { ImportMode } from '#lib/bindings/ImportMode.ts';
	import type { ImportSummary } from '#lib/bindings/ImportSummary.ts';
	import type { Overview } from '#lib/bindings/Overview.ts';
	import { describeConflict, describeImport } from '#lib/format.ts';

	let { today, onImported }: { today: string; onImported: (overview: Overview) => void } =
		$props();

	const filters = [{ name: 'Epicycle backup', extensions: ['json'] }];

	// An import chosen but not yet confirmed, with what it would do.
	let pending = $state<{ path: string; mode: ImportMode; summary: ImportSummary } | null>(null);
	let message = $state<string | null>(null);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let panel: HTMLElement | undefined = $state();

	// The preview opens below the buttons, possibly off screen: bring it into view.
	$effect(() => {
		if (pending) panel?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
	});

	function fileName(path: string): string {
		return path.split(/[\\/]/).pop() ?? path;
	}

	// Runs an action, showing its error if it fails.
	async function run(action: () => Promise<void>) {
		busy = true;
		error = null;
		message = null;
		try {
			await action();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	const exportFile = () =>
		run(async () => {
			const path = await save({ defaultPath: `epicycle-backup-${today}.json`, filters });
			if (!path) return;
			await exportBackup(path);
			message = `Saved ${fileName(path)}.`;
		});

	const chooseFile = () =>
		run(async () => {
			const path = await open({ multiple: false, directory: false, filters });
			if (!path) return;
			pending = null;
			pending = { path, mode: 'merge', summary: await previewImport(path, 'merge') };
		});

	const changeMode = (mode: ImportMode) =>
		run(async () => {
			if (!pending) return;
			const { path } = pending;
			pending = { path, mode, summary: await previewImport(path, mode) };
		});

	const confirmImport = () =>
		run(async () => {
			if (!pending) return;
			const { path, mode } = pending;
			const result = await importBackup(path, mode);
			pending = null;
			onImported(result.overview);
			message = [
				...describeImport(result.summary, true),
				...result.summary.conflicts.map((c) => describeConflict(c, true) + '.')
			].join(' ');
		});
</script>

<section aria-labelledby="backup-heading">
	<h2 id="backup-heading">Backup</h2>
	<p class="muted">
		Save every subscription and its history to a file, or bring them in from one.
	</p>
	<div class="actions">
		<button class="secondary" onclick={exportFile} disabled={busy}>Export…</button>
		<button class="secondary" onclick={chooseFile} disabled={busy}>Import…</button>
	</div>

	{#if pending}
		<div class="panel" bind:this={panel}>
			<h3>Import {fileName(pending.path)}</h3>
			<fieldset>
				<legend>How to import</legend>
				<label>
					<input
						type="radio"
						name="import-mode"
						checked={pending.mode === 'merge'}
						onchange={() => changeMode('merge')}
						disabled={busy}
					/>
					<span>
						Merge
						<span class="hint">Add what's new in the backup and keep changes made here.</span>
					</span>
				</label>
				<label>
					<input
						type="radio"
						name="import-mode"
						checked={pending.mode === 'replace'}
						onchange={() => changeMode('replace')}
						disabled={busy}
					/>
					<span>
						Replace
						<span class="hint">
							Make this device match the backup. What it replaces stays in each subscription's
							history.
						</span>
					</span>
				</label>
			</fieldset>

			<ul>
				{#each describeImport(pending.summary, false) as line (line)}
					<li>{line}</li>
				{/each}
			</ul>
			{#if pending.summary.conflicts.length > 0}
				<p>Changed both here and in the backup since they last matched:</p>
				<ul>
					{#each pending.summary.conflicts as conflict, i (i)}
						<li>{describeConflict(conflict, false)}</li>
					{/each}
				</ul>
				<p class="hint">The other version stays in the subscription's history.</p>
			{/if}

			<div class="actions">
				<button class="secondary" onclick={() => (pending = null)} disabled={busy}>Cancel</button>
				<button
					class={pending.mode === 'replace' ? 'danger' : ''}
					onclick={confirmImport}
					disabled={busy}
				>
					{pending.mode === 'replace' ? 'Replace' : 'Merge'}
				</button>
			</div>
		</div>
	{/if}

	{#if message}
		<p role="status">{message}</p>
	{/if}
	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}
</section>

<style>
	section {
		display: grid;
		gap: 0.75rem;
	}

	h2 {
		margin: 0;
		font-size: 1rem;
		color: var(--muted);
		font-weight: 600;
	}

	h3 {
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

	.hint {
		display: block;
		color: var(--muted);
		font-size: 0.875rem;
	}

	.error {
		color: var(--danger);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}

	.panel {
		display: grid;
		gap: 0.75rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 1rem;
	}

	.panel .actions {
		justify-content: flex-end;
	}

	fieldset {
		border: 0;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.5rem;
	}

	legend {
		padding: 0;
		margin-bottom: 0.5rem;
		font-weight: 500;
	}

	label {
		display: flex;
		gap: 0.75rem;
		align-items: flex-start;
		min-height: 44px;
		cursor: pointer;
	}

	input[type='radio'] {
		width: 1.25rem;
		height: 1.25rem;
		min-height: 0;
		margin: 0.125rem 0 0;
		flex-shrink: 0;
		accent-color: var(--accent);
	}

	ul {
		margin: 0;
		padding-left: 1.25rem;
	}
</style>
