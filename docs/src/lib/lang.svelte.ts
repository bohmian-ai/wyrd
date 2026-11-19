import { browser } from '$app/environment';

// The multi-language device: one shared choice (Python, Rust, or TypeScript) that every
// LangTabs block on every page follows. Rune-based reactive module so any
// component that reads `lang.value` re-renders when the choice changes;
// persisted to localStorage under `wyrd:lang`.

export type Lang = 'python' | 'rust' | 'typescript';

const KEY = 'wyrd:lang';

let value = $state<Lang>('python');
let loaded = false;

function read(): Lang | undefined {
  if (!browser) return undefined;
  try {
    const v = localStorage.getItem(KEY);
    return LANGS.find((l) => l.id === v)?.id;
  } catch {
    return undefined;
  }
}

// Hydrate once from localStorage (called from the layout on mount). SSR/first
// paint default is `python`; this flips it to the persisted choice after mount.
export function initLang(): void {
  if (loaded) return;
  loaded = true;
  const v = read();
  if (v) value = v;
}

export const lang = {
  get value(): Lang {
    return value;
  },
  set(v: Lang): void {
    value = v;
    if (browser) {
      try {
        localStorage.setItem(KEY, v);
      } catch {
        /* storage unavailable — keep the in-memory choice */
      }
    }
  }
};

export const LANGS: { id: Lang; label: string }[] = [
  { id: 'python', label: 'Python' },
  { id: 'rust', label: 'Rust' },
  { id: 'typescript', label: 'TypeScript' }
];
