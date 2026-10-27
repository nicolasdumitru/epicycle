// Pure display helpers. Every function takes an explicit locale so tests are
// deterministic; the UI passes `undefined` to use the device's locale.

import type { Billing } from './bindings/Billing';
import type { MoneyView } from './bindings/MoneyView';

type Locale = string | undefined;

/** "€12.99", using the currency's own number of decimals. */
export function formatMoney(money: MoneyView, locale: Locale): string {
	// Display only: the exact value stays a decimal string everywhere else.
	return new Intl.NumberFormat(locale, { style: 'currency', currency: money.currency }).format(
		Number(money.amount)
	);
}

/** A calendar date from an ISO "YYYY-MM-DD" string, without time-zone shifts. */
export function parseDate(iso: string): Date {
	const [year, month, day] = iso.split('-').map(Number);
	return new Date(year ?? 0, (month ?? 1) - 1, day ?? 1);
}

export function formatDate(iso: string, locale: Locale): string {
	return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(parseDate(iso));
}

/** Whole days from `from` to `to` (negative if `to` is earlier). */
export function daysBetween(from: string, to: string): number {
	const ms = parseDate(to).getTime() - parseDate(from).getTime();
	// Round to absorb daylight-saving shifts of an hour.
	return Math.round(ms / 86_400_000);
}

/** "today", "tomorrow", "in 12 days". */
export function relativeDays(days: number, locale: Locale): string {
	return new Intl.RelativeTimeFormat(locale, { numeric: 'auto' }).format(days, 'day');
}

function ordinal(day: number): string {
	const suffix =
		day % 10 === 1 && day !== 11
			? 'st'
			: day % 10 === 2 && day !== 12
				? 'nd'
				: day % 10 === 3 && day !== 13
					? 'rd'
					: 'th';
	return `${day}${suffix}`;
}

function dayOfMonth(day: number): string {
	// Day 31 falls on the last day of every month (see DECISIONS.md).
	return day === 31 ? 'the last day' : `the ${ordinal(day)}`;
}

function every(n: number, singular: string, plural: string): string {
	return n === 1 ? singular : `Every ${n} ${plural}`;
}

/**
 * "Monthly on the 15th", "Every 2 weeks on Friday", "Yearly on 15 March".
 * Weekly billing has no stored weekday; it is the next payment's weekday.
 */
export function describeBilling(billing: Billing, nextPayment: string | null, locale: Locale): string {
	switch (billing.unit) {
		case 'days':
			return every(billing.every, 'Daily', 'days');
		case 'weeks': {
			const base = every(billing.every, 'Weekly', 'weeks');
			if (nextPayment === null) return base;
			const weekday = new Intl.DateTimeFormat(locale, { weekday: 'long' }).format(
				parseDate(nextPayment)
			);
			return `${base} on ${weekday}`;
		}
		case 'months':
			return `${every(billing.every, 'Monthly', 'months')} on ${dayOfMonth(billing.day)}`;
		case 'years': {
			// Any leap year works, so that February 29 can be formatted.
			const date = new Date(2024, billing.month - 1, billing.day);
			const label = new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'long' }).format(
				date
			);
			return `${every(billing.every, 'Yearly', 'years')} on ${label}`;
		}
	}
}

/** True if the billing period is a month already, so a monthly equivalent adds nothing. */
export function isMonthly(billing: Billing): boolean {
	return billing.unit === 'months' && billing.every === 1;
}
