import type { Session, TenantContext } from '$lib/server/auth/session';
import type { WyrdClient } from '$lib/server/wyrd';
import type { WyrdProblem } from '$lib/views';

declare global {
  namespace App {
    interface Locals {
      mockData: boolean;
      session: Session | null;
      sessionProblem: WyrdProblem | null;
      tenant?: TenantContext;
      wyrd?: WyrdClient;
    }
    interface Error extends WyrdProblem {
      message: string;
    }
  }
}

export {};
