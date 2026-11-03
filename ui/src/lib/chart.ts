// Pure helpers for the hand-written SVG charts (see docs/DECISIONS.md: charts
// are SVG until a library earns its place).

/**
 * Round tick values from 0 up to at least `max`: steps of 1, 2, 2.5 or 5 times a
 * power of ten, about `count` of them. Always at least [0, 1].
 */
export function niceTicks(max: number, count = 4): number[] {
	if (!(max > 0)) return [0, 1];
	const rough = max / count;
	const power = 10 ** Math.floor(Math.log10(rough));
	const step = [1, 2, 2.5, 5, 10].map((m) => m * power).find((s) => s >= rough) ?? 10 * power;
	const top = Math.ceil(max / step) * step;
	const ticks: number[] = [];
	// Integer steps avoid floating-point drift (0.1 + 0.2 …).
	for (let i = 0; i * step <= top + step / 1e9; i++) ticks.push(Number((i * step).toPrecision(12)));
	return ticks;
}

/**
 * An SVG path for a column from `baseline` up to `top` (smaller y), with the
 * data end rounded and the baseline end square. Empty for a zero-height bar.
 */
export function columnPath(x: number, width: number, top: number, baseline: number, radius = 4): string {
	const height = baseline - top;
	if (height <= 0 || width <= 0) return '';
	const r = Math.min(radius, width / 2, height);
	return [
		`M${x},${baseline}`,
		`V${top + r}`,
		`Q${x},${top} ${x + r},${top}`,
		`H${x + width - r}`,
		`Q${x + width},${top} ${x + width},${top + r}`,
		`V${baseline}`,
		'Z'
	].join(' ');
}
