// Typed wrappers around the Tauri commands in crates/app/src/commands.rs.
// They reject with an error message.

import { invoke } from '@tauri-apps/api/core';
import type { ConvertedTotal } from './bindings/ConvertedTotal';
import type { CurrencyOption } from './bindings/CurrencyOption';
import type { DeletedView } from './bindings/DeletedView';
import type { Edit } from './bindings/Edit';
import type { Imported } from './bindings/Imported';
import type { ImportMode } from './bindings/ImportMode';
import type { ImportSummary } from './bindings/ImportSummary';
import type { NewSubscription } from './bindings/NewSubscription';
import type { Overview } from './bindings/Overview';
import type { RatesOverview } from './bindings/RatesOverview';
import type { RevisionView } from './bindings/RevisionView';
import type { SubscriptionDetail } from './bindings/SubscriptionDetail';

export const getOverview = (): Promise<Overview> => invoke('overview');

/** Currencies offered for new subscriptions; the list never changes, so it is fetched once. */
let currencyList: Promise<CurrencyOption[]> | undefined;
export const getCurrencies = (): Promise<CurrencyOption[]> =>
	(currencyList ??= invoke<CurrencyOption[]>('currencies'));

export const addSubscription = (form: NewSubscription): Promise<Overview> =>
	invoke('add_subscription', { form });

export const getSubscription = (id: string): Promise<SubscriptionDetail> =>
	invoke('subscription', { id });

export const editSubscription = (id: string, edit: Edit): Promise<SubscriptionDetail> =>
	invoke('edit_subscription', { id, edit });

export const deleteSubscription = (id: string): Promise<Overview> =>
	invoke('delete_subscription', { id });

export const exportBackup = (path: string): Promise<void> => invoke('export_backup', { path });

/** What importing the backup at `path` would do, without changing anything. */
export const previewImport = (path: string, mode: ImportMode): Promise<ImportSummary> =>
	invoke('preview_import', { path, mode });

export const importBackup = (path: string, mode: ImportMode): Promise<Imported> =>
	invoke('import_backup', { path, mode });

/** A subscription's change history, newest first; works for deleted ones too. */
export const getHistory = (id: string): Promise<RevisionView[]> =>
	invoke('subscription_history', { id });

/** Makes an earlier version current again, recording a new revision. */
export const restoreRevision = (id: string, revision: string): Promise<SubscriptionDetail> =>
	invoke('restore_revision', { id, revision });

export const getDeleted = (): Promise<DeletedView[]> => invoke('deleted_subscriptions');

/** The current commitment in `currency`, or in the reporting currency if null. */
export const getConvertedTotal = (currency: string | null): Promise<ConvertedTotal> =>
	invoke('converted_total', { currency });

/** Fetches new ECB rates if the cached ones are out of date; how many were stored. */
export const refreshRates = (): Promise<number> => invoke('refresh_rates');

export const getRates = (): Promise<RatesOverview> => invoke('exchange_rates');

export const addManualRate = (
	currency: string,
	date: string,
	perEur: string
): Promise<RatesOverview> => invoke('add_manual_rate', { currency, date, perEur });

export const deleteManualRate = (currency: string, date: string): Promise<RatesOverview> =>
	invoke('delete_manual_rate', { currency, date });

export const setReportingCurrency = (currency: string): Promise<void> =>
	invoke('set_reporting_currency', { currency });
