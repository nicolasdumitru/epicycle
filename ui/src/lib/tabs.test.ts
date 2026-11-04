import { describe, expect, it } from 'vitest';
import { tabFor } from './tabs';

describe('tabFor', () => {
	it('keeps detail pages under their section', () => {
		expect(tabFor('/')).toBe('/');
		expect(tabFor('/subscriptions')).toBe('/subscriptions');
		expect(tabFor('/subscriptions/0199-abc')).toBe('/subscriptions');
		expect(tabFor('/deleted')).toBe('/subscriptions');
		expect(tabFor('/more')).toBe('/more');
		expect(tabFor('/rates')).toBe('/more');
		expect(tabFor('/reminders')).toBe('/more');
	});
});
