import { mockHome } from './mock';
import type { HomeView } from '$lib/views';
import { reject, type TenantContext } from './auth/session';

/** The single server-only domain seam. Mock projections are not durable domain records. */
export class WyrdClient {
  constructor(
    private readonly context: TenantContext,
    private readonly mockData: boolean
  ) {}

  home(): HomeView {
    if (
      ['cards:read', 'bifrost_query:read', 'evals:read'].some(
        (permission) => !this.context.permissions.includes(permission)
      )
    )
      reject('denied');
    // No fallback to fixtures: the live transport must be connected here before server mode can serve data.
    if (!this.mockData) reject('upstream');
    return mockHome(this.context.tenant);
  }
}
