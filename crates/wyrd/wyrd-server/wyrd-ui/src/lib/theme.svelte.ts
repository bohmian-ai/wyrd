import { browser } from '$app/environment';

export type Mode = 'light' | 'dark';

/** localStorage key persisting the viewer's mode choice across reloads and deep links. */
const storageKey = 'wyrd-mode';

/**
 * Reads the persisted mode choice, falling back to dark for first visits and
 * during server-side rendering.
 */
function initialMode(): Mode {
  if (!browser) return 'dark';
  const stored = localStorage.getItem(storageKey);
  return stored === 'light' || stored === 'dark' ? stored : 'dark';
}

// App-wide active mode. Shared, reactive ($state) — import and mutate `theme.mode`.
// ponytail: SSR always renders dark, so a persisted light choice repaints on hydration;
// move the choice to a cookie if the flash ever matters.
export const theme = $state<{ mode: Mode }>({ mode: initialMode() });

/** Flips the app-wide mode and persists the choice for future visits. */
export function toggleMode(): void {
  theme.mode = theme.mode === 'dark' ? 'light' : 'dark';
  if (browser) localStorage.setItem(storageKey, theme.mode);
}
