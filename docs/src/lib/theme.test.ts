import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { setTheme } from './theme.js';

// The pre-paint bootstrap is the inline <script> in app.html, run before any
// stylesheet or bundle. Execute that exact source against a minimal browser
// stand-in so the test proves what ships, not a copy of it.
const appHtml = readFileSync(new URL('../app.html', import.meta.url), 'utf8');
const bootstrap = appHtml.match(/<script>([\s\S]*?)<\/script>/)?.[1] ?? '';

type Root = { dataset: Record<string, string>; style: Record<string, string> };

function fakeBrowser(stored: string | null, prefersDark: boolean) {
  const store = new Map<string, string>(stored === null ? [] : [['wyrd:theme', stored]]);
  const root: Root = { dataset: {}, style: {} };
  const localStorage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v)
  };
  const window = { matchMedia: () => ({ matches: prefersDark }) };
  const document = { documentElement: root };
  return { root, store, localStorage, window, document };
}

function boot(stored: string | null, prefersDark: boolean): Root {
  const b = fakeBrowser(stored, prefersDark);
  new Function('localStorage', 'window', 'document', bootstrap)(b.localStorage, b.window, b.document);
  return b.root;
}

describe('pre-paint theme bootstrap', () => {
  it('follows a dark system preference on a first visit', () => {
    const root = boot(null, true);
    expect(root.dataset.theme).toBe('dark');
    expect(root.style.colorScheme).toBe('dark');
  });

  it('follows a light system preference on a first visit', () => {
    const root = boot(null, false);
    expect(root.dataset.theme).toBe('light');
    expect(root.style.colorScheme).toBe('light');
  });

  it('applies a persisted explicit choice over the system preference', () => {
    const root = boot('light', true);
    expect(root.dataset.theme).toBe('light');
    expect(root.style.colorScheme).toBe('light');
  });

  it('ignores an unrecognized stored value', () => {
    expect(boot('arcade', true).dataset.theme).toBe('dark');
  });
});

describe('setTheme', () => {
  it('applies the choice, the native color scheme, and persists it for the next visit', () => {
    const b = fakeBrowser(null, true);
    setTheme('light', b.document.documentElement, b.localStorage);
    expect(b.root.dataset.theme).toBe('light');
    expect(b.root.style.colorScheme).toBe('light');

    const next = boot(b.store.get('wyrd:theme') ?? null, true);
    expect(next.dataset.theme).toBe('light');
  });

  it('still applies the choice when storage is unavailable', () => {
    const b = fakeBrowser(null, false);
    const blocked = {
      setItem: () => {
        throw new Error('blocked');
      }
    };
    setTheme('dark', b.document.documentElement, blocked);
    expect(b.root.dataset.theme).toBe('dark');
  });
});
