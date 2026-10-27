import { describe, expect, it } from 'vitest';
import {
	daysBetween,
	describeBilling,
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
