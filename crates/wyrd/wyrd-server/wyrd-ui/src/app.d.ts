import type { Session, TenantContext } from '$lib/server/auth/session';
import type { ServerSession } from '$lib/server/auth/server-sessions';
import type { WyrdClient } from '$lib/server/wyrd';
import type { WyrdProblem } from '$lib/views';

declare global {
  namespace App {
    interface Locals {
      mockData: boolean;
      /** Development-only local identity; always null in production. */
      session: Session | null;
      /** Production server-owned session projection; holds no session id or token. */
      serverSession?: ServerSession;
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
