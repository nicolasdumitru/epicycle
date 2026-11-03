// Typed wrappers around the Tauri commands in crates/app/src/commands.rs.
// They reject with an error message.

import { invoke } from '@tauri-apps/api/core';
import type { CurrencyOption } from './bindings/CurrencyOption';
import type { Imported } from './bindings/Imported';
import type { ImportMode } from './bindings/ImportMode';
import type { ImportSummary } from './bindings/ImportSummary';
import type { NewSubscription } from './bindings/NewSubscription';
import type { Overview } from './bindings/Overview';

export const getOverview = (): Promise<Overview> => invoke('overview');

/** Currencies offered for new subscriptions; the list never changes, so it is fetched once. */
let currencyList: Promise<CurrencyOption[]> | undefined;
export const getCurrencies = (): Promise<CurrencyOption[]> =>
	(currencyList ??= invoke<CurrencyOption[]>('currencies'));

export const addSubscription = (form: NewSubscription): Promise<Overview> =>
	invoke('add_subscription', { form });

export const deleteSubscription = (id: string): Promise<Overview> =>
	invoke('delete_subscription', { id });

export const exportBackup = (path: string): Promise<void> => invoke('export_backup', { path });

/** What importing the backup at `path` would do, without changing anything. */
export const previewImport = (path: string, mode: ImportMode): Promise<ImportSummary> =>
	invoke('preview_import', { path, mode });

export const importBackup = (path: string, mode: ImportMode): Promise<Imported> =>
	invoke('import_backup', { path, mode });
