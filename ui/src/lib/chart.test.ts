import { describe, expect, it } from 'vitest';
import { columnPath, niceTicks } from './chart';

describe('niceTicks', () => {
	it('uses round steps that reach the maximum', () => {
		expect(niceTicks(87)).toEqual([0, 25, 50, 75, 100]);
		expect(niceTicks(120)).toEqual([0, 50, 100, 150]);
		expect(niceTicks(9.5)).toEqual([0, 2.5, 5, 7.5, 10]);
		expect(niceTicks(0.3)).toEqual([0, 0.1, 0.2, 0.3]);
	});

	it('copes with nothing to show', () => {
		expect(niceTicks(0)).toEqual([0, 1]);
		expect(niceTicks(Number.NaN)).toEqual([0, 1]);
	});
});

describe('columnPath', () => {
	it('rounds the top and keeps the base square', () => {
		expect(columnPath(10, 20, 50, 100)).toBe(
			'M10,100 V54 Q10,50 14,50 H26 Q30,50 30,54 V100 Z'
		);
	});

	it('draws nothing for a zero-height column', () => {
		expect(columnPath(10, 20, 100, 100)).toBe('');
	});
});
