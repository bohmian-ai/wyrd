import type { Session, TenantContext } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import type { WyrdClient } from '$lib/server/wyrd';
import type { WyrdProblem } from '$lib/views';

declare global {
  namespace App {
    interface Locals {
      mockData: boolean;
      /** Development-only local identity; always null in production. */
      session: Session | null;
      /** Production tenant session; its access token is private to the object. */
      browserSession?: BrowserSession;
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
