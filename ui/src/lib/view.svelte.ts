// View state that outlives a page, for the rest of the session: switching tabs
// keeps the currency the overview shows totals in.
export const view = $state({
	/** The currency totals and the chart are shown in; empty until loaded. */
	currency: '',
	/** Goes up when exchange rates or the default currency change. */
	ratesVersion: 0
});
