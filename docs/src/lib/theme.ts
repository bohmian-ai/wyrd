// Docs theme is local presentation state only. The pre-paint bootstrap in
// app.html resolves the first theme (stored choice, else system preference);
// this applies an explicit choice afterwards and persists it under the same key.
export type Theme = 'light' | 'dark';

export const THEME_KEY = 'wyrd:theme';

type Root = { dataset: DOMStringMap; style: { colorScheme: string } };

export function setTheme(
  theme: Theme,
  root: Root = document.documentElement,
  storage: Pick<Storage, 'setItem'> = localStorage
): void {
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  try {
    storage.setItem(THEME_KEY, theme);
  } catch {
    // Storage blocked: the choice still applies for this visit.
  }
}
