import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/environment', () => ({ browser: true }));

const store = new Map<string, string>();
vi.stubGlobal('localStorage', {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v)
});

describe('shared language choice', () => {
  beforeEach(() => {
    store.clear();
    vi.resetModules();
  });

  it('offers Python, Rust, and TypeScript in that order', async () => {
    const { LANGS } = await import('./lang.svelte');
    expect(LANGS.map((l) => l.id)).toEqual(['python', 'rust', 'typescript']);
  });

  it('persists a TypeScript choice for every later block and visit', async () => {
    const first = await import('./lang.svelte');
    first.lang.set('typescript');
    expect(store.get('wyrd:lang')).toBe('typescript');

    vi.resetModules();
    const later = await import('./lang.svelte');
    later.initLang();
    expect(later.lang.value).toBe('typescript');
  });

  it('ignores an unknown stored value and keeps the Python default', async () => {
    store.set('wyrd:lang', 'go');
    const { initLang, lang } = await import('./lang.svelte');
    initLang();
    expect(lang.value).toBe('python');
  });
});
