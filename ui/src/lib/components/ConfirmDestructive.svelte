<script lang="ts">
	// A modal confirmation for actions that can't be undone. The red confirm
	// button stays disabled for a few seconds, with a ring counting down, so it
	// can't be clicked by reflex; Cancel looks friendly and has focus.
	import type { Snippet } from 'svelte';

	let {
		title,
		confirmLabel,
		seconds = 5,
		busy = false,
		onConfirm,
		onCancel,
		children
	}: {
		title: string;
		confirmLabel: string;
		seconds?: number;
		busy?: boolean;
		onConfirm: () => void;
		onCancel: () => void;
		children: Snippet;
	} = $props();

	const id = $props.id();
	let dialog: HTMLDialogElement | undefined = $state();
	let cancelButton: HTMLButtonElement | undefined = $state();
	// Seconds left before the confirm button works.
	// svelte-ignore state_referenced_locally (the delay is fixed when the dialog opens)
	let remaining = $state(seconds);

	const radius = 9;
	const circumference = 2 * Math.PI * radius;

	$effect(() => {
		dialog?.showModal();
		cancelButton?.focus();
		const start = performance.now();
		const timer = setInterval(() => {
			remaining = Math.max(0, seconds - (performance.now() - start) / 1000);
			if (remaining === 0) clearInterval(timer);
		}, 50);
		return () => clearInterval(timer);
	});

	function oncancel(event: Event) {
		// Escape: close through the parent, which owns whether this is shown.
		event.preventDefault();
		if (!busy) onCancel();
	}
</script>

<dialog bind:this={dialog} {oncancel} aria-labelledby="{id}-title" aria-describedby="{id}-body">
	<div class="icon" aria-hidden="true">!</div>
	<h2 id="{id}-title">{title}</h2>
	<div id="{id}-body" class="body">
		{@render children()}
	</div>
	<div class="actions">
		<button bind:this={cancelButton} class="cancel" onclick={onCancel} disabled={busy}>
			Cancel, keep my data
		</button>
		<button class="danger" onclick={onConfirm} disabled={remaining > 0 || busy}>
			{#if remaining > 0}
				<svg class="ring" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
					<circle cx="12" cy="12" r={radius} class="track" />
					<circle
						cx="12"
						cy="12"
						r={radius}
						class="progress"
						stroke-dasharray={circumference}
						stroke-dashoffset={circumference * (1 - remaining / seconds)}
					/>
					<text x="12" y="12">{Math.ceil(remaining)}</text>
				</svg>
				<span class="visually-hidden">Available in {Math.ceil(remaining)} seconds</span>
			{/if}
			{busy ? 'Working…' : confirmLabel}
		</button>
	</div>
</dialog>

<style>
	dialog {
		width: min(28rem, calc(100vw - 2rem));
		box-sizing: border-box;
		padding: 1.25rem;
		border: 2px solid var(--danger);
		border-radius: 1rem;
		background: var(--surface);
		color: var(--fg);
	}

	/* Only when open: a closed dialog must stay hidden. */
	dialog[open] {
		display: grid;
		gap: 0.875rem;
	}

	dialog::backdrop {
		background: rgb(0 0 0 / 0.5);
	}

	.icon {
		width: 2.5rem;
		height: 2.5rem;
		border-radius: 50%;
		display: grid;
		place-items: center;
		background: var(--danger);
		color: var(--surface);
		font-weight: 800;
		font-size: 1.375rem;
	}

	h2 {
		margin: 0;
		font-size: 1.25rem;
	}

	.body {
		display: grid;
		gap: 0.5rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap-reverse;
		justify-content: flex-end;
		gap: 0.75rem;
		margin-top: 0.25rem;
	}

	/* Friendly and prominent: the safe choice. */
	.cancel {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-fg);
	}

	.danger {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
	}

	.ring {
		transform: rotate(-90deg);
		flex: none;
	}

	.ring circle {
		fill: none;
		stroke-width: 2.5;
	}

	.track {
		stroke: currentColor;
		opacity: 0.3;
	}

	.progress {
		stroke: currentColor;
		stroke-linecap: round;
	}

	.ring text {
		transform: rotate(90deg);
		transform-origin: 12px 12px;
		fill: currentColor;
		font-size: 11px;
		font-weight: 700;
		text-anchor: middle;
		dominant-baseline: central;
	}

	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}
</style>
