<script lang="ts">
	// A date field that accepts typing ("2026-11-03", or "3.11.2026" in the
	// locale's order) and has a calendar for picking. Used instead of
	// <input type="date">, whose typing doesn't work in WebKitGTK and which makes
	// jumping to a distant month slow.
	import {
		addMonths,
		calendarWeeks,
		firstDayOfWeek,
		parseTypedDate,
		toIso
	} from '#lib/inputs.ts';
	import { formatDate, parseDate } from '#lib/format.ts';

	let {
		value = $bindable(),
		label,
		hint = '',
		required = false,
		today
	}: {
		/** ISO "YYYY-MM-DD", or "" when empty. */
		value: string;
		label: string;
		hint?: string;
		required?: boolean;
		/** ISO date the calendar opens on when the field is empty. */
		today: string;
	} = $props();

	// `undefined` means the device's locale.
	const locale = undefined;
	const id = $props.id();
	const firstDay = firstDayOfWeek(locale);

	// svelte-ignore state_referenced_locally (only the initial value is wanted)
	let text = $state(value);
	let open = $state(false);
	let view = $state({ year: 2000, month: 1 });
	let input: HTMLInputElement | undefined = $state();
	let root: HTMLDivElement | undefined = $state();

	const parsed = $derived(text.trim() === '' ? null : parseTypedDate(text, locale));
	const invalid = $derived(text.trim() !== '' && parsed === null);
	const weeks = $derived(calendarWeeks(view.year, view.month, firstDay));

	const weekdayNames = Array.from({ length: 7 }, (_, i) =>
		// 2024-01-01 was a Monday (ISO day 1).
		new Intl.DateTimeFormat(locale, { weekday: 'narrow' }).format(
			new Date(2024, 0, ((firstDay - 1 + i) % 7) + 1)
		)
	);
	const monthNames = Array.from({ length: 12 }, (_, i) =>
		new Intl.DateTimeFormat(locale, { month: 'long' }).format(new Date(2024, i, 1))
	);

	// The value this field last reported or was given. Only a different value
	// counts as a change from outside (e.g. a form reset) and replaces the
	// text, so a half-typed date is never overwritten while typing.
	// svelte-ignore state_referenced_locally (compared, not tracked)
	let lastValue = value;
	$effect(() => {
		if (value !== lastValue) {
			lastValue = value;
			text = value;
		}
	});

	$effect(() => {
		input?.setCustomValidity(
			invalid ? 'Enter a real date, like 2026-11-03' : required && text.trim() === '' ? 'Enter a date' : ''
		);
	});

	function oninput(event: Event) {
		// Read the field itself: the binding to `text` may not have run yet.
		const typed = (event.currentTarget as HTMLInputElement).value;
		text = typed;
		value = lastValue = typed.trim() === '' ? '' : (parseTypedDate(typed, locale) ?? '');
	}

	function onblur() {
		// Show what was understood, in one unambiguous format.
		if (parsed) text = parsed;
	}

	function toggle() {
		if (!open) {
			const start = parseDate(value || today);
			view = { year: start.getFullYear(), month: start.getMonth() + 1 };
		}
		open = !open;
	}

	function pick(day: number) {
		value = lastValue = toIso(view.year, view.month, day);
		text = value;
		open = false;
		input?.focus();
	}

	function step(delta: number) {
		view = addMonths(view.year, view.month, delta);
	}

	function setYear(event: Event) {
		const year = Number((event.target as HTMLInputElement).value);
		if (Number.isInteger(year) && year >= 1000 && year <= 9999) view = { ...view, year };
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			event.preventDefault();
			event.stopPropagation();
			open = false;
			input?.focus();
		}
	}

	function onfocusout(event: FocusEvent) {
		// Close the calendar once focus leaves the whole field.
		if (!root?.contains(event.relatedTarget as Node | null)) open = false;
	}

	// Escape closes the calendar, and so does moving focus out of the field.
	// Attached here rather than as attributes because the wrapper isn't itself
	// an interactive element.
	$effect(() => {
		const el = root;
		if (!el) return;
		el.addEventListener('keydown', onkeydown);
		el.addEventListener('focusout', onfocusout);
		return () => {
			el.removeEventListener('keydown', onkeydown);
			el.removeEventListener('focusout', onfocusout);
		};
	});

	const selectedIn = (day: number) => value === toIso(view.year, view.month, day);
	const isToday = (day: number) => today === toIso(view.year, view.month, day);
</script>

<div class="field" bind:this={root}>
	<label for={id}>
		{label}
		{#if !required}<span class="optional">(optional)</span>{/if}
	</label>
	<div class="row">
		<input
			{id}
			bind:this={input}
			bind:value={text}
			{oninput}
			{onblur}
			{required}
			placeholder="YYYY-MM-DD"
			inputmode="numeric"
			autocomplete="off"
			spellcheck="false"
			aria-invalid={invalid}
			aria-describedby="{id}-hint"
		/>
		<button
			type="button"
			class="secondary icon"
			onclick={toggle}
			aria-expanded={open}
			aria-controls="{id}-calendar"
			aria-label="Choose {label.toLowerCase()} from a calendar"
		>
			<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
				<rect x="3" y="5" width="18" height="16" rx="2" fill="none" stroke="currentColor" stroke-width="2" />
				<path d="M3 10h18M8 3v4M16 3v4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
			</svg>
		</button>
	</div>
	<span id="{id}-hint" class="hint" class:invalid>
		{#if invalid}
			Not a date. Type it like 2026-11-03.
		{:else if parsed}
			{formatDate(parsed, locale)}{hint ? ` · ${hint}` : ''}
		{:else}
			{hint || ' '}
		{/if}
	</span>

	{#if open}
		<div id="{id}-calendar" class="calendar" role="dialog" aria-label="Choose {label.toLowerCase()}">
			<div class="nav">
				<button type="button" class="secondary icon" onclick={() => step(-1)} aria-label="Previous month">‹</button>
				<select
					aria-label="Month"
					value={view.month}
					onchange={(e) => (view = { ...view, month: Number((e.target as HTMLSelectElement).value) })}
				>
					{#each monthNames as name, i (i)}
						<option value={i + 1}>{name}</option>
					{/each}
				</select>
				<input
					class="year"
					type="number"
					min="1000"
					max="9999"
					aria-label="Year"
					value={view.year}
					oninput={setYear}
				/>
				<button type="button" class="secondary icon" onclick={() => step(1)} aria-label="Next month">›</button>
			</div>
			<table>
				<thead>
					<tr>
						{#each weekdayNames as name, i (i)}
							<th scope="col">{name}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each weeks as week, w (w)}
						<tr>
							{#each week as day, d (d)}
								<td>
									{#if day !== null}
										<button
											type="button"
											class="day"
											class:selected={selectedIn(day)}
											class:today={isToday(day)}
											aria-pressed={selectedIn(day)}
											aria-label={formatDate(toIso(view.year, view.month, day), locale)}
											onclick={() => pick(day)}>{day}</button
										>
									{/if}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
			<div class="footer">
				<button type="button" class="link" onclick={() => {
					const t = parseDate(today);
					view = { year: t.getFullYear(), month: t.getMonth() + 1 };
				}}>Go to today</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.field {
		position: relative;
		display: grid;
		gap: 0.375rem;
		font-weight: 500;
	}

	.row {
		display: flex;
		gap: 0.5rem;
	}

	.icon {
		flex: none;
		width: 44px;
		padding: 0;
		display: grid;
		place-items: center;
		font-size: 1.5rem;
		line-height: 1;
	}

	.hint,
	.optional {
		color: var(--muted);
		font-size: 0.875rem;
		font-weight: 400;
	}

	.hint.invalid {
		color: var(--danger);
	}

	.calendar {
		position: absolute;
		z-index: 10;
		top: calc(100% + 0.25rem);
		left: 0;
		width: min(22rem, calc(100vw - 2rem));
		box-sizing: border-box;
		padding: 0.75rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
		font-weight: 400;
	}

	.nav {
		display: flex;
		gap: 0.375rem;
	}

	.nav select {
		flex: 1;
		min-width: 0;
		padding: 0 0.375rem;
	}

	.year {
		width: 5.5rem;
		flex: none;
		padding: 0 0.375rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		margin-top: 0.5rem;
		table-layout: fixed;
	}

	th {
		color: var(--muted);
		font-size: 0.75rem;
		font-weight: 500;
		padding: 0.25rem 0;
	}

	td {
		padding: 1px;
		text-align: center;
	}

	.day {
		width: 100%;
		min-height: 40px;
		padding: 0;
		background: transparent;
		border: 1px solid transparent;
		color: var(--fg);
		font-weight: 400;
	}

	.day:hover {
		border-color: var(--border);
	}

	.day.today {
		border-color: var(--accent);
	}

	.day.selected {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-fg);
		font-weight: 600;
	}

	.footer {
		display: flex;
		justify-content: center;
		margin-top: 0.25rem;
	}
</style>
