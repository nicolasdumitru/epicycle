import { describe, expect, it } from 'vitest';
import {
	addMonths,
	calendarWeeks,
	findCurrency,
	matchCurrencies,
	monthFirst,
	parseTypedDate
} from './inputs';

const options = [
	{ code: 'EUR', name: 'Euro' },
	{ code: 'RON', name: 'Romanian leu' },
	{ code: 'RUB', name: 'Russian ruble' },
	{ code: 'MDL', name: 'Moldovan leu' }
];

describe('currencies', () => {
	it('puts the exact code first, then code prefixes, then names', () => {
		expect(matchCurrencies(options, 'ron').map((o) => o.code)).toEqual(['RON']);
		expect(matchCurrencies(options, 'r').map((o) => o.code)).toEqual(['RON', 'RUB', 'EUR']);
		expect(matchCurrencies(options, 'leu').map((o) => o.code)).toEqual(['RON', 'MDL']);
		expect(matchCurrencies(options, '  ')).toEqual(options);
		expect(matchCurrencies(options, 'xyz')).toEqual([]);
	});

	it('finds a code regardless of case and spaces', () => {
		expect(findCurrency(options, ' ron ')?.code).toBe('RON');
		expect(findCurrency(options, 'RO')).toBeUndefined();
	});
});

describe('typed dates', () => {
	it('reads year-first dates with any separator', () => {
		expect(parseTypedDate('2026-11-03', 'en-GB')).toBe('2026-11-03');
		expect(parseTypedDate('2026/1/5', 'en-US')).toBe('2026-01-05');
		expect(parseTypedDate(' 2026 2 28 ', undefined)).toBe('2026-02-28');
	});

	it('reads year-last dates in the locale order', () => {
		expect(monthFirst('en-US')).toBe(true);
		expect(monthFirst('ro-RO')).toBe(false);
		expect(parseTypedDate('3.11.2026', 'ro-RO')).toBe('2026-11-03');
		expect(parseTypedDate('11/3/2026', 'en-US')).toBe('2026-11-03');
	});

	it('rejects dates that do not exist', () => {
		expect(parseTypedDate('2026-02-29', 'en-GB')).toBeNull();
		expect(parseTypedDate('2028-02-29', 'en-GB')).toBe('2028-02-29');
		expect(parseTypedDate('2026-13-01', 'en-GB')).toBeNull();
		expect(parseTypedDate('31.04.2026', 'ro-RO')).toBeNull();
		expect(parseTypedDate('26-11-03', 'en-GB')).toBeNull();
		expect(parseTypedDate('tomorrow', 'en-GB')).toBeNull();
		expect(parseTypedDate('', 'en-GB')).toBeNull();
	});
});

describe('calendar', () => {
	it('lays out a month starting on the given weekday', () => {
		// 1 November 2026 is a Sunday.
		const monday = calendarWeeks(2026, 11, 1);
		expect(monday[0]).toEqual([null, null, null, null, null, null, 1]);
		expect(monday.at(-1)).toEqual([30, null, null, null, null, null, null]);
		const sunday = calendarWeeks(2026, 11, 7);
		expect(sunday[0]).toEqual([1, 2, 3, 4, 5, 6, 7]);
		expect(sunday.flat().filter((d) => d !== null)).toHaveLength(30);
	});

	it('steps across years', () => {
		expect(addMonths(2026, 12, 1)).toEqual({ year: 2027, month: 1 });
		expect(addMonths(2026, 1, -1)).toEqual({ year: 2025, month: 12 });
		expect(addMonths(2026, 3, -27)).toEqual({ year: 2023, month: 12 });
	});
});
