// The app's sections, shown as tabs: a bar at the bottom on phones and a row at
// the top on wider screens (see routes/+layout.svelte).

export type Tab = { href: string; label: string };

export const tabs: Tab[] = [
	{ href: '/', label: 'Overview' },
	{ href: '/subscriptions', label: 'Subscriptions' },
	{ href: '/more', label: 'More' }
];

/** The tab a page belongs to, so detail pages keep their section highlighted. */
export function tabFor(path: string): string {
	if (path.startsWith('/subscriptions') || path.startsWith('/deleted')) return '/subscriptions';
	if (path.startsWith('/more') || path.startsWith('/rates')) return '/more';
	return '/';
}
