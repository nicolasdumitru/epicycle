import { describe, expect, it } from 'vitest';
import {
	backupFileName,
	describeRateSource,
	formatRate,
	describeChange,
	daysBetween,
	describeBilling,
	describeConflict,
	describeImport,
	formatDate,
	formatMoney,
	isMonthly,
	relativeDays
} from './format';

const en = 'en-GB';

describe('formatMoney', () => {
	it('uses the currency symbol and its decimals', () => {
		expect(formatMoney({ amount: '12.99', currency: 'EUR' }, en)).toBe('€12.99');
		expect(formatMoney({ amount: '10.00', currency: 'USD' }, 'en-US')).toBe('$10.00');
		expect(formatMoney({ amount: '1500', currency: 'JPY' }, en)).toBe('JP¥1,500');
	});
});

describe('dates', () => {
	it('formats ISO dates without shifting the day', () => {
		expect(formatDate('2026-03-01', en)).toBe('1 Mar 2026');
	});

	it('counts days across month ends and daylight-saving changes', () => {
		expect(daysBetween('2026-10-27', '2026-10-31')).toBe(4);
		expect(daysBetween('2026-03-28', '2026-03-30')).toBe(2);
		expect(daysBetween('2026-10-27', '2026-10-27')).toBe(0);
		expect(daysBetween('2027-01-01', '2026-12-31')).toBe(-1);
	});

	it('describes relative days', () => {
		expect(relativeDays(0, en)).toBe('today');
		expect(relativeDays(1, en)).toBe('tomorrow');
		expect(relativeDays(12, en)).toBe('in 12 days');
	});
});

describe('describeBilling', () => {
	it('describes every unit', () => {
		expect(describeBilling({ unit: 'days', every: 1 }, null, en)).toBe('Daily');
		expect(describeBilling({ unit: 'days', every: 30 }, null, en)).toBe('Every 30 days');
		// 2026-10-30 is a Friday.
		expect(describeBilling({ unit: 'weeks', every: 2 }, '2026-10-30', en)).toBe(
			'Every 2 weeks on Friday'
		);
		expect(describeBilling({ unit: 'weeks', every: 1 }, null, en)).toBe('Weekly');
		expect(describeBilling({ unit: 'months', every: 1, day: 15 }, null, en)).toBe(
			'Monthly on the 15th'
		);
		expect(describeBilling({ unit: 'months', every: 3, day: 1 }, null, en)).toBe(
			'Every 3 months on the 1st'
		);
		expect(describeBilling({ unit: 'years', every: 1, month: 3, day: 15 }, null, en)).toBe(
			'Yearly on 15 March'
		);
		expect(describeBilling({ unit: 'years', every: 2, month: 2, day: 29 }, null, en)).toBe(
			'Every 2 years on 29 February'
		);
	});

	it('calls day 31 the last day and handles teen ordinals', () => {
		expect(describeBilling({ unit: 'months', every: 1, day: 31 }, null, en)).toBe(
			'Monthly on the last day'
		);
		expect(describeBilling({ unit: 'months', every: 1, day: 12 }, null, en)).toBe(
			'Monthly on the 12th'
		);
		expect(describeBilling({ unit: 'months', every: 1, day: 22 }, null, en)).toBe(
			'Monthly on the 22nd'
		);
	});

	it('knows when a monthly equivalent is redundant', () => {
		expect(isMonthly({ unit: 'months', every: 1, day: 1 })).toBe(true);
		expect(isMonthly({ unit: 'months', every: 3, day: 1 })).toBe(false);
		expect(isMonthly({ unit: 'weeks', every: 1 })).toBe(false);
	});
});

describe('describeImport', () => {
	const none = { added: 0, updated: 0, removed: 0, conflicts: [], lostRevisions: 0 };

	it('lists each kind of change, in the right tense', () => {
		const summary = { ...none, added: 2, removed: 1 };
		expect(describeImport(summary, false)).toEqual(['Adds 2 subscriptions.', 'Removes 1 subscription.']);
		expect(describeImport(summary, true)).toEqual(['Added 2 subscriptions.', 'Removed 1 subscription.']);
	});

	it('warns about history that would be lost', () => {
		expect(describeImport({ ...none, lostRevisions: 3 }, false)).toEqual([
			"Permanently deletes 3 changes in this device's history that the backup doesn't have."
		]);
	});

	it('says when there is nothing to do', () => {
		expect(describeImport(none, false)).toEqual([
			'Nothing to change: this device already has everything in the backup.'
		]);
	});

	it('names the side kept in a conflict', () => {
		expect(describeConflict({ name: 'Music', kept: 'backup', keptDeleted: false }, false)).toBe(
			"Music: keeps the backup's version"
		);
		expect(describeConflict({ name: 'Music', kept: 'device', keptDeleted: true }, true)).toBe(
			"Music: kept this device's deletion"
		);
	});
});

describe('backupFileName', () => {
	it('includes the local date and time, padded', () => {
		expect(backupFileName(new Date(2026, 10, 3, 9, 5, 7))).toBe(
			'epicycle-backup-2026-11-03_09-05-07.json'
		);
	});
});

describe('describeChange', () => {
	it('describes each kind of change', () => {
		expect(describeChange({ kind: 'renamed', from: 'Music', to: 'Music Family' }, en)).toBe(
			'Renamed from “Music” to “Music Family”'
		);
		expect(describeChange({ kind: 'notes', notes: '' }, en)).toBe('Notes removed');
		expect(
			describeChange(
				{
					kind: 'plan_set',
					from: '2026-12-01',
					price: { amount: '12.50', currency: 'EUR' },
					billing: { unit: 'months', every: 1, day: 15 },
					anchor: '2026-12-15'
				},
				en
			)
		).toBe('From 1 Dec 2026: €12.50, monthly on the 15th');
		expect(describeChange({ kind: 'status_set', from: '2027-01-01', status: 'paused' }, en)).toBe(
			'Paused from 1 Jan 2027'
		);
	});
});

describe('rates', () => {
	const rate = {
		currency: 'RON',
		date: '2026-11-02',
		perEur: '5.3405',
		source: 'ecb' as const,
		freshness: 'fresh' as const
	};

	it('shows the rate and where it came from', () => {
		expect(formatRate(rate, en)).toBe('1 EUR = 5.3405 RON');
		expect(describeRateSource(rate, en)).toBe('ECB, 2 Nov 2026');
		expect(describeRateSource({ ...rate, freshness: 'stale' }, en)).toBe(
			'ECB, 2 Nov 2026 (out of date)'
		);
		expect(describeRateSource({ ...rate, source: 'manual', freshness: 'manual' }, en)).toBe(
			'entered by you, 2 Nov 2026'
		);
	});
});
