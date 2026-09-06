import { browser } from '$app/environment';

export type Mode = 'light' | 'dark';

/** Cookie persisting the viewer's mode choice — readable by the server so SSR paints the right theme on first byte. */
const cookieName = 'wyrd-mode';

/**
 * Reads the persisted mode choice from the cookie, falling back to dark for
 * first visits. During SSR the root layout overwrites this with the request's
 * cookie value, so the module default only covers the client bootstrap.
 */
function initialMode(): Mode {
  if (!browser) return 'dark';
  const match = document.cookie.match(/(?:^|; )wyrd-mode=(light|dark)/);
  return match ? (match[1] as Mode) : 'dark';
}

// App-wide active mode. Shared, reactive ($state) — import and mutate `theme.mode`.
export const theme = $state<{ mode: Mode }>({ mode: initialMode() });

/** Flips the app-wide mode and persists the choice for future visits. */
export function toggleMode(): void {
  theme.mode = theme.mode === 'dark' ? 'light' : 'dark';
  if (browser) {
    document.cookie = `${cookieName}=${theme.mode}; path=/; max-age=31536000; samesite=lax`;
  }
}
