// Pure display helpers. Every function takes an explicit locale so tests are
// deterministic; the UI passes `undefined` to use the device's locale.

import type { Billing } from './bindings/Billing';
import type { ChangeView } from './bindings/ChangeView';
import type { ConflictView } from './bindings/ConflictView';
import type { ImportSummary } from './bindings/ImportSummary';
import type { MoneyView } from './bindings/MoneyView';
import type { StatusChoice } from './bindings/StatusChoice';
import type { Unit } from './bindings/Unit';

type Locale = string | undefined;

/** Billing units with their singular and plural names, for "every n …" pickers. */
export const billingUnits: { value: Unit; one: string; many: string }[] = [
	{ value: 'days', one: 'day', many: 'days' },
	{ value: 'weeks', one: 'week', many: 'weeks' },
	{ value: 'months', one: 'month', many: 'months' },
	{ value: 'years', one: 'year', many: 'years' }
];

export const statusChoiceLabels: Record<StatusChoice, string> = {
	active: 'Active',
	paused: 'Paused',
	cancelled: 'Cancelled'
};

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
	// Day 31 falls on the last day of every month (see docs/DECISIONS.md).
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

function subscriptions(n: number): string {
	return n === 1 ? '1 subscription' : `${n} subscriptions`;
}

/**
 * One sentence per kind of change an import makes: "Adds 2 subscriptions."
 * `done` switches to the past tense, for after the import.
 */
export function describeImport(summary: ImportSummary, done: boolean): string[] {
	const verbs = done
		? { added: 'Added', updated: 'Updated', removed: 'Removed' }
		: { added: 'Adds', updated: 'Updates', removed: 'Removes' };
	const lines = (['added', 'updated', 'removed'] as const)
		.filter((kind) => summary[kind] > 0)
		.map((kind) => `${verbs[kind]} ${subscriptions(summary[kind])}.`);
	if (summary.lostRevisions > 0) {
		const n = summary.lostRevisions;
		const changes = n === 1 ? '1 change' : `${n} changes`;
		lines.push(
			`${done ? 'Permanently deleted' : 'Permanently deletes'} ${changes} in this device's history that the backup doesn't have.`
		);
	}
	if (lines.length === 0 && summary.conflicts.length === 0) {
		lines.push(
			done
				? 'Nothing changed: this device already had everything in the backup.'
				: 'Nothing to change: this device already has everything in the backup.'
		);
	}
	return lines;
}

/** "Music: keeps the backup's version" for a subscription changed on both sides. */
export function describeConflict(conflict: ConflictView, done: boolean): string {
	const verb = done ? 'kept' : 'keeps';
	const side = conflict.kept === 'device' ? "this device's" : "the backup's";
	const what = conflict.keptDeleted ? 'deletion' : 'version';
	return `${conflict.name}: ${verb} ${side} ${what}`;
}

/**
 * "epicycle-backup-2026-11-03_19-45-07.json", in local time. Hyphens rather
 * than colons, which Windows doesn't allow in file names.
 */
export function backupFileName(now: Date): string {
	const p = (n: number) => String(n).padStart(2, '0');
	const date = `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
	const time = `${p(now.getHours())}-${p(now.getMinutes())}-${p(now.getSeconds())}`;
	return `epicycle-backup-${date}_${time}.json`;
}

/** "3 Nov 2026, 19:45", in local time, from an RFC 3339 timestamp. */
export function formatTimestamp(rfc3339: string, locale: Locale): string {
	return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(
		new Date(rfc3339)
	);
}

/** One line per change in a subscription's history. */
export function describeChange(change: ChangeView, locale: Locale): string {
	switch (change.kind) {
		case 'created':
			return 'Created';
		case 'recreated':
			return 'Restored after being deleted';
		case 'deleted':
			return 'Deleted';
		case 'renamed':
			return `Renamed from “${change.from}” to “${change.to}”`;
		case 'notes':
			return change.notes === '' ? 'Notes removed' : 'Notes changed';
		case 'plan_set':
			return `From ${formatDate(change.from, locale)}: ${formatMoney(change.price, locale)}, ${describeBilling(change.billing, change.anchor, locale).toLowerCase()}`;
		case 'plan_removed':
			return `Price change from ${formatDate(change.from, locale)} removed`;
		case 'status_set': {
			const what = { active: 'Resumed', paused: 'Paused', cancelled: 'Cancelled' }[change.status];
			return `${what} from ${formatDate(change.from, locale)}`;
		}
		case 'status_removed':
			return `Status change from ${formatDate(change.from, locale)} removed`;
	}
}
