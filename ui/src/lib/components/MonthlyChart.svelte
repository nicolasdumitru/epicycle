<script lang="ts">
	// Month by month: what is paid (columns: scheduled cash flow) and the
	// monthly equivalent of what is active (line: commitment), on one axis in
	// one currency. Kept visibly apart, as the spec asks: an annual plan is one
	// tall column but a flat line. Months before this one are estimates.
	//
	// Colours are categorical slots 1 and 2 of the dataviz reference palette,
	// validated against this app's light (#ffffff) and dark (#20201e) surfaces.
	import { getMonthlyChart } from '#lib/api.ts';
	import type { MonthlyChart } from '#lib/bindings/MonthlyChart.ts';
	import type { MonthView } from '#lib/bindings/MonthView.ts';
	import { columnPath, niceTicks } from '#lib/chart.ts';
	import { formatMoney } from '#lib/format.ts';

	let {
		currency,
		reloadKey
	}: {
		/** Empty until the default currency is known. */
		currency: string;
		reloadKey: unknown;
	} = $props();

	// `undefined` means the device's locale.
	const locale = undefined;
	const height = 220;
	const margin = { top: 24, right: 12, bottom: 28, left: 52 };

	let chart = $state<MonthlyChart | null>(null);
	let error = $state<string | null>(null);
	let width = $state(600);
	// The month under the pointer or keyboard focus.
	let active = $state<number | null>(null);

	$effect(() => {
		void reloadKey;
		const asked = currency;
		if (!asked) return;
		getMonthlyChart(asked)
			.then((c) => {
				if (asked === currency) chart = c;
				error = null;
			})
			.catch((e) => (error = String(e)));
	});

	const months = $derived(chart?.months ?? []);
	const plotWidth = $derived(Math.max(width - margin.left - margin.right, 1));
	const plotHeight = height - margin.top - margin.bottom;
	const baseline = margin.top + plotHeight;
	const band = $derived(plotWidth / Math.max(months.length, 1));
	const barWidth = $derived(Math.min(24, band * 0.6));
	const ticks = $derived(
		niceTicks(
			Math.max(0, ...months.map((m) => Math.max(Number(m.cashFlow.amount), Number(m.commitment.amount))))
		)
	);
	const top = $derived(ticks.at(-1) ?? 1);
	// Every other month label when they would crowd each other.
	const labelEvery = $derived(band < 40 ? 2 : 1);
	const firstScheduled = $derived(months.findIndex((m) => !m.estimated));
	// The one column labelled directly: the biggest scheduled month, the one
	// worth planning for (or the biggest past one if nothing is scheduled).
	const biggest = $derived.by(() => {
		const scheduled = months.map((m, i) => ({ m, i })).filter(({ m }) => !m.estimated);
		const pool = scheduled.length > 0 ? scheduled : months.map((m, i) => ({ m, i }));
		return pool.reduce(
			(best, c) => (Number(c.m.cashFlow.amount) > Number(best.m.cashFlow.amount) ? c : best),
			pool[0] ?? { m: undefined as unknown as MonthView, i: 0 }
		).i;
	});

	const y = (value: number) => baseline - (value / top) * plotHeight;
	const centre = (i: number) => margin.left + band * i + band / 2;

	const linePoints = $derived(
		months.map((m, i) => `${centre(i)},${y(Number(m.commitment.amount))}`).join(' ')
	);

	function monthLabel(m: MonthView, i: number): string {
		const date = new Date(`${m.month}T00:00:00`);
		const withYear = i === 0 || date.getMonth() === 0;
		return new Intl.DateTimeFormat(locale, withYear ? { month: 'short', year: '2-digit' } : { month: 'short' }).format(
			date
		);
	}

	function monthName(m: MonthView): string {
		return new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric' }).format(
			new Date(`${m.month}T00:00:00`)
		);
	}

	function axisLabel(value: number): string {
		if (!chart) return String(value);
		return new Intl.NumberFormat(locale, {
			style: 'currency',
			currency: chart.currency,
			notation: top >= 10_000 ? 'compact' : 'standard',
			maximumFractionDigits: top < 10 ? 1 : 0
		}).format(value);
	}

	function describe(m: MonthView): string {
		const paid = formatMoney(m.cashFlow, locale);
		const rate = formatMoney(m.commitment, locale);
		const kind = m.estimated ? 'estimated' : 'scheduled';
		return `${monthName(m)}: ${paid} paid (${kind}); ${rate} monthly equivalent`;
	}

	const activeMonth = $derived(active === null ? undefined : months[active]);

	// Keeps the tooltip inside the chart.
	const tooltipLeft = $derived(
		active === null ? 0 : Math.min(Math.max(centre(active) - 90, 0), Math.max(width - 180, 0))
	);
</script>

<section class="chart" aria-labelledby="chart-heading">
	<h2 id="chart-heading">Month by month</h2>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{:else if chart && chart.missing.length > 0}
		<p class="muted">
			No exchange rate for {chart.missing.join(', ')}, so the chart can't be drawn.
			<a href="/rates">Enter a rate</a>
		</p>
	{:else if chart}
		<div class="card">
			<ul class="legend" aria-label="Legend">
				<li><span class="swatch bar" aria-hidden="true"></span>Paid in the month</li>
				<li><span class="swatch line" aria-hidden="true"></span>Monthly equivalent</li>
			</ul>

			<div class="plot" bind:clientWidth={width}>
				<svg {width} {height} role="img" aria-label="Payments and monthly equivalent per month, in {chart.currency}. The table below has the values.">
					{#each ticks as tick (tick)}
						<line class="grid" x1={margin.left} x2={width - margin.right} y1={y(tick)} y2={y(tick)} />
						<text class="axis" x={margin.left - 6} y={y(tick)} text-anchor="end" dominant-baseline="middle">
							{axisLabel(tick)}
						</text>
					{/each}

					{#if firstScheduled > 0}
						{@const x = margin.left + band * firstScheduled}
						<line class="divider" x1={x} x2={x} y1={margin.top - 14} y2={baseline} />
						<text class="axis" x={x - 6} y={margin.top - 8} text-anchor="end">Estimated</text>
						<text class="axis" x={x + 6} y={margin.top - 8}>Scheduled</text>
					{/if}

					{#each months as m, i (m.month)}
						<path
							class="column"
							class:estimated={m.estimated}
							d={columnPath(centre(i) - barWidth / 2, barWidth, y(Number(m.cashFlow.amount)), baseline)}
						/>
						{#if i % labelEvery === 0}
							<text class="axis" x={centre(i)} y={baseline + 18} text-anchor="middle">{monthLabel(m, i)}</text>
						{/if}
					{/each}

					<polyline class="commitment" points={linePoints} />
					{#each months as m, i (m.month)}
						<circle class="marker" cx={centre(i)} cy={y(Number(m.commitment.amount))} r="4" />
					{/each}

					<!-- One selective direct label; the axis, legend, tooltip and table carry the rest. -->
					{#if months[biggest] && Number(months[biggest].cashFlow.amount) > 0}
						<text class="value" x={centre(biggest)} y={y(Number(months[biggest].cashFlow.amount)) - 6} text-anchor="middle">
							{formatMoney(months[biggest].cashFlow, locale)}
						</text>
					{/if}

					<line class="baseline" x1={margin.left} x2={width - margin.right} y1={baseline} y2={baseline} />

					{#if active !== null && months[active]}
						<line class="crosshair" x1={centre(active)} x2={centre(active)} y1={margin.top} y2={baseline} />
					{/if}

					<!-- Hit targets: a whole month's column of the plot, bigger than its marks. -->
					{#each months as m, i (m.month)}
						<rect
							class="hit"
							x={margin.left + band * i}
							y={margin.top}
							width={band}
							height={plotHeight}
							role="button"
							tabindex="0"
							aria-label={describe(m)}
							onpointerenter={() => (active = i)}
							onpointerleave={() => (active = null)}
							onfocus={() => (active = i)}
							onblur={() => (active = null)}
						/>
					{/each}
				</svg>

				{#if activeMonth}
					{@const m = activeMonth}
					<div class="tooltip" style:left="{tooltipLeft}px" aria-hidden="true">
						<p class="title">{monthName(m)}</p>
						<p>
							<span class="swatch bar"></span>{formatMoney(m.cashFlow, locale)} paid
							<span class="muted">({m.estimated ? 'estimated' : 'scheduled'})</span>
						</p>
						<p><span class="swatch line"></span>{formatMoney(m.commitment, locale)} monthly equivalent</p>
					</div>
				{/if}
			</div>

			<p class="note">
				Columns are what is paid in each month: a yearly plan is one tall column. The line is the
				monthly equivalent of what is active, which only changes when subscriptions do. Past months
				are estimated from your plans, not from your bank.
			</p>

			<details>
				<summary>Show as a table</summary>
				<table>
					<thead>
						<tr>
							<th scope="col">Month</th>
							<th scope="col">Paid</th>
							<th scope="col">Monthly equivalent</th>
						</tr>
					</thead>
					<tbody>
						{#each months as m (m.month)}
							<tr>
								<th scope="row">{monthName(m)}{m.estimated ? ' (estimated)' : ''}</th>
								<td>{formatMoney(m.cashFlow, locale)}</td>
								<td>{formatMoney(m.commitment, locale)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</details>
		</div>
	{:else}
		<p class="muted">Loading…</p>
	{/if}
</section>

<style>
	.chart {
		--series-1: #2a78d6;
		--series-2: #eb6834;
		--grid: #ececea;
		--axis-line: #c3c2b7;
		display: grid;
		gap: 0.5rem;
	}

	@media (prefers-color-scheme: dark) {
		.chart {
			--series-1: #3987e5;
			--series-2: #d95926;
			--grid: #2c2c29;
			--axis-line: #383835;
		}

		/* Lighter than in light mode: the faded columns need to stay visible on
		   the dark surface. (`.chart` makes it outrank the rule below.) */
		.chart .column.estimated {
			fill-opacity: 0.6;
		}
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

	.muted,
	.note {
		color: var(--muted);
	}

	.note {
		font-size: 0.875rem;
	}

	.error {
		color: var(--danger);
	}

	a {
		color: var(--accent);
	}

	.card {
		display: grid;
		gap: 0.75rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		padding: 0.875rem 1rem;
		min-width: 0;
	}

	.legend {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1rem;
		font-size: 0.875rem;
	}

	.legend li {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.swatch {
		display: inline-block;
		flex: none;
		margin-right: 0.375rem;
		vertical-align: middle;
	}

	.swatch.bar {
		width: 10px;
		height: 10px;
		border-radius: 2px;
		background: var(--series-1);
	}

	.swatch.line {
		width: 14px;
		height: 2px;
		border-radius: 1px;
		background: var(--series-2);
	}

	.plot {
		position: relative;
		min-width: 0;
	}

	svg {
		display: block;
		overflow: visible;
	}

	.grid {
		stroke: var(--grid);
		stroke-width: 1;
	}

	.baseline {
		stroke: var(--axis-line);
		stroke-width: 1;
	}

	.divider {
		stroke: var(--axis-line);
		stroke-width: 1;
	}

	.crosshair {
		stroke: var(--axis-line);
		stroke-width: 1;
	}

	.axis {
		fill: var(--muted);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
	}

	.value {
		fill: var(--fg);
		font-size: 11px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.column {
		fill: var(--series-1);
	}

	.column.estimated {
		fill-opacity: 0.45;
	}

	.commitment {
		fill: none;
		stroke: var(--series-2);
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}

	.marker {
		fill: var(--series-2);
		stroke: var(--surface);
		stroke-width: 2;
	}

	.hit {
		fill: transparent;
		outline: none;
		cursor: default;
	}

	.hit:focus-visible {
		fill: var(--grid);
		fill-opacity: 0.5;
	}

	.tooltip {
		position: absolute;
		top: 0;
		width: 180px;
		box-sizing: border-box;
		padding: 0.5rem 0.625rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 0.5rem;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
		font-size: 0.8125rem;
		pointer-events: none;
		display: grid;
		gap: 0.125rem;
	}

	.tooltip .title {
		font-weight: 600;
	}

	summary {
		cursor: pointer;
		padding: 0.5rem 0;
		color: var(--muted);
		font-size: 0.875rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
		font-variant-numeric: tabular-nums;
	}

	th,
	td {
		text-align: right;
		padding: 0.25rem 0.5rem;
		border-bottom: 1px solid var(--border);
	}

	th[scope='row'],
	thead th:first-child {
		text-align: left;
		font-weight: 400;
	}
</style>
