export type Mode = 'light' | 'dark';

// App-wide active mode. Shared, reactive ($state) — import and mutate `theme.mode`.
export const theme = $state<{ mode: Mode }>({ mode: 'dark' });

export function toggleMode(): void {
  theme.mode = theme.mode === 'dark' ? 'light' : 'dark';
}
