import type { Session, TenantContext } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import type { PlatformSession } from '$lib/server/auth/platform-sessions';
import type { WyrdProblem } from '$lib/server/problem';

declare global {
  namespace App {
    interface Locals {
      /** Development-only local identity; always null in production. */
      session: Session | null;
      /** Production tenant session; its access token is private to the object. */
      browserSession?: BrowserSession;
      /** Platform console session on `/platform/ui`; its cookie is private to the object. */
      platformSession?: PlatformSession;
      sessionProblem: WyrdProblem | null;
      tenant?: TenantContext;
    }
    interface Error extends WyrdProblem {
      message: string;
    }
  }
}

export {};
