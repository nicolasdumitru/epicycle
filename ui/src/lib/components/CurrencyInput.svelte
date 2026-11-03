<script lang="ts">
	// A combobox for ISO 4217 currencies: type a code or part of a name, pick
	// from the list. Only listed currencies are accepted, and a code typed in any
	// case ("ron") becomes the listed one ("RON").
	import type { CurrencyOption } from '#lib/bindings/CurrencyOption.ts';
	import { findCurrency, matchCurrencies } from '#lib/inputs.ts';

	let {
		value = $bindable(),
		options,
		label
	}: { value: string; options: CurrencyOption[]; label: string } = $props();

	const id = $props.id();
	// svelte-ignore state_referenced_locally (only the initial value is wanted)
	let text = $state(value);
	let open = $state(false);
	let active = $state(0);
	let input: HTMLInputElement | undefined = $state();

	const matches = $derived(matchCurrencies(options, text).slice(0, 50));
	const selected = $derived(findCurrency(options, value));

	// Block form submission until a listed currency is chosen.
	$effect(() => {
		input?.setCustomValidity(
			selected ? '' : text.trim() === '' ? 'Choose a currency' : `Unknown currency "${text.trim()}"`
		);
	});

	function choose(option: CurrencyOption) {
		value = option.code;
		text = option.code;
		open = false;
	}

	function oninput() {
		open = true;
		active = 0;
		// An exact code is chosen as soon as it's typed, without needing Enter.
		value = findCurrency(options, text)?.code ?? '';
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
			event.preventDefault();
			if (!open) {
				open = true;
				return;
			}
			const step = event.key === 'ArrowDown' ? 1 : -1;
			active = (active + step + matches.length) % Math.max(matches.length, 1);
			document.getElementById(`${id}-option-${active}`)?.scrollIntoView({ block: 'nearest' });
		} else if (event.key === 'Enter' && open) {
			const option = matches[active];
			if (!option) return;
			// Choose instead of submitting the form.
			event.preventDefault();
			choose(option);
		} else if (event.key === 'Escape' && open) {
			event.preventDefault();
			open = false;
		}
	}

	function onblur() {
		open = false;
		const exact = findCurrency(options, text);
		// A unique match by name counts too: "leu" alone is ambiguous, "romanian" isn't.
		const only = matches.length === 1 ? matches[0] : undefined;
		const pick = exact ?? only;
		if (pick) choose(pick);
	}
</script>

<div class="field">
	<label for={id}>{label}</label>
	<div class="box">
		<input
			{id}
			bind:this={input}
			bind:value={text}
			{oninput}
			{onkeydown}
			{onblur}
			onfocus={() => input?.select()}
			onclick={() => (open = true)}
			role="combobox"
			aria-expanded={open}
			aria-controls="{id}-list"
			aria-autocomplete="list"
			aria-activedescendant={open && matches[active] ? `${id}-option-${active}` : undefined}
			aria-describedby="{id}-name"
			autocomplete="off"
			autocapitalize="characters"
			spellcheck="false"
			required
		/>
		{#if open && matches.length > 0}
			<ul id="{id}-list" role="listbox" aria-label={label}>
				{#each matches as option, i (option.code)}
					<li
						id="{id}-option-{i}"
						role="option"
						aria-selected={i === active}
						class:active={i === active}
						onpointerdown={(e) => {
							// Keep focus in the input so blur doesn't close the list first.
							e.preventDefault();
							choose(option);
						}}
					>
						<span class="code">{option.code}</span>
						<span class="name">{option.name}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
	<span id="{id}-name" class="hint" class:invalid={!selected && !open && text.trim() !== ''}>
		{selected ? selected.name : text.trim() === '' || open ? ' ' : 'Not a currency'}
	</span>
</div>

<style>
	.field {
		display: grid;
		gap: 0.375rem;
		font-weight: 500;
	}

	.box {
		position: relative;
	}

	input {
		text-transform: uppercase;
	}

	ul {
		position: absolute;
		z-index: 10;
		top: calc(100% + 0.25rem);
		right: 0;
		width: max(100%, 16rem);
		max-height: 16rem;
		overflow-y: auto;
		margin: 0;
		padding: 0.25rem;
		list-style: none;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.5rem;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
	}

	li {
		display: flex;
		gap: 0.75rem;
		align-items: center;
		min-height: 44px;
		padding: 0 0.5rem;
		border-radius: 0.375rem;
		cursor: pointer;
		font-weight: 400;
	}

	li.active {
		background: var(--accent);
		color: var(--accent-fg);
	}

	.code {
		font-weight: 600;
		width: 3rem;
	}

	.hint {
		color: var(--muted);
		font-size: 0.875rem;
		font-weight: 400;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.hint.invalid {
		color: var(--danger);
	}
</style>
