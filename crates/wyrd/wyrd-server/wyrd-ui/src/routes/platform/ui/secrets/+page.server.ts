import { consoleChange } from '$lib/server/auth/platform-sessions';
import type { Actions } from './$types';

/** What one routine maintenance pass observed and did (`StoredSecretMaintenanceReport`); never key or secret material. */
export type MaintenanceReport = {
  current: number;
  changed: number;
  remaining: number;
  bounded: boolean;
  failures: { store: string; data_tenant_id: string | null; owner: string; reason: string }[];
};

export const actions: Actions = {
  /** Rewrap every stored secret not yet under the active wrapping key; repeat while the report says more remain. */
  run: ({ locals }) =>
    consoleChange(async () => ({
      report: await locals.platformSession!.json<MaintenanceReport>('POST', '/stored-secrets/maintenance', { mode: 'routine' })
    }))
};
