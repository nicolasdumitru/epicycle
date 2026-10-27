// Typed wrappers around the Tauri commands in crates/app/src/commands.rs.
// Each one resolves to the refreshed overview, or rejects with an error message.

import { invoke } from '@tauri-apps/api/core';
import type { NewSubscription } from './bindings/NewSubscription';
import type { Overview } from './bindings/Overview';

export const getOverview = (): Promise<Overview> => invoke('overview');

export const addSubscription = (form: NewSubscription): Promise<Overview> =>
	invoke('add_subscription', { form });

export const deleteSubscription = (id: string): Promise<Overview> =>
	invoke('delete_subscription', { id });
