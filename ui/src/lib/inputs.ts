// Pure logic behind the currency and date inputs, kept out of the components so
// it can be tested.

import type { CurrencyOption } from './bindings/CurrencyOption';

type Locale = string | undefined;

/**
 * Currencies matching what was typed, best first: the exact code, then codes
 * starting with it, then names containing it ("leu" finds RON).
 */
export function matchCurrencies(options: CurrencyOption[], query: string): CurrencyOption[] {
	const q = query.trim().toLowerCase();
	if (q === '') return options;
	const rank = (o: CurrencyOption): number => {
		const code = o.code.toLowerCase();
		if (code === q) return 0;
		if (code.startsWith(q)) return 1;
		if (o.name.toLowerCase().includes(q)) return 2;
		return -1;
	};
	return options
		.map((o) => ({ o, r: rank(o) }))
		.filter(({ r }) => r >= 0)
		.sort((a, b) => a.r - b.r)
		.map(({ o }) => o);
}

/** The option whose code is exactly `text`, ignoring case and spaces. */
export function findCurrency(options: CurrencyOption[], text: string): CurrencyOption | undefined {
	const code = text.trim().toUpperCase();
	return options.find((o) => o.code === code);
}

function pad(n: number, width: number): string {
	return String(n).padStart(width, '0');
}

export function toIso(year: number, month: number, day: number): string {
	return `${pad(year, 4)}-${pad(month, 2)}-${pad(day, 2)}`;
}

export function daysInMonth(year: number, month: number): number {
	// Day 0 of the next month is the last day of this one.
	return new Date(year, month, 0).getDate();
}

function valid(year: number, month: number, day: number): string | null {
	if (year < 1000 || year > 9999 || month < 1 || month > 12) return null;
	if (day < 1 || day > daysInMonth(year, month)) return null;
	return toIso(year, month, day);
}

/** True if the locale writes the month before the day (as en-US does). */
export function monthFirst(locale: Locale): boolean {
	const parts = new Intl.DateTimeFormat(locale, {
		year: 'numeric',
		month: '2-digit',
		day: '2-digit'
	}).formatToParts(new Date(2026, 10, 3));
	const types = parts.map((p) => p.type);
	return types.indexOf('month') < types.indexOf('day');
}

/**
 * A typed date as ISO "YYYY-MM-DD", or null if it isn't a real date.
 *
 * Year first ("2026-11-3", "2026/11/03") always works. With the year last
 * ("3.11.2026"), day and month are read in the locale's usual order. Any of
 * - / . or spaces separate the parts.
 */
export function parseTypedDate(text: string, locale: Locale): string | null {
	const parts = text.trim().split(/[-/.\s]+/);
	if (parts.length !== 3 || !parts.every((p) => /^\d+$/.test(p))) return null;
	const [a, b, c] = parts as [string, string, string];
	if (a.length === 4) return valid(Number(a), Number(b), Number(c));
	if (c.length === 4) {
		const [month, day] = monthFirst(locale) ? [a, b] : [b, a];
		return valid(Number(c), Number(month), Number(day));
	}
	return null;
}

/**
 * The days of a month laid out in weeks, for a calendar grid. Days before the
 * 1st and after the last are null. `firstDay` is the first day of the week,
 * 1 = Monday … 7 = Sunday (ISO numbering).
 */
export function calendarWeeks(year: number, month: number, firstDay: number): (number | null)[][] {
	// getDay(): 0 = Sunday … 6 = Saturday; convert to ISO 1–7.
	const weekday = new Date(year, month - 1, 1).getDay() || 7;
	const lead = (weekday - firstDay + 7) % 7;
	const cells: (number | null)[] = Array.from({ length: lead }, () => null);
	for (let d = 1; d <= daysInMonth(year, month); d++) cells.push(d);
	while (cells.length % 7 !== 0) cells.push(null);
	const weeks: (number | null)[][] = [];
	for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
	return weeks;
}

/** The locale's first day of the week (1 = Monday … 7 = Sunday); Monday if unknown. */
export function firstDayOfWeek(locale: Locale): number {
	try {
		const loc = new Intl.Locale(locale ?? new Intl.DateTimeFormat().resolvedOptions().locale);
		// `getWeekInfo` is newer than the `weekInfo` getter; support both.
		const info = (
			loc as unknown as {
				getWeekInfo?: () => { firstDay: number };
				weekInfo?: { firstDay: number };
			}
		).getWeekInfo?.() ?? (loc as unknown as { weekInfo?: { firstDay: number } }).weekInfo;
		return info?.firstDay ?? 1;
	} catch {
		return 1;
	}
}

/** The month `delta` months after (or before) year/month. */
export function addMonths(year: number, month: number, delta: number): { year: number; month: number } {
	const index = year * 12 + (month - 1) + delta;
	return { year: Math.floor(index / 12), month: (index % 12) + 1 };
}
