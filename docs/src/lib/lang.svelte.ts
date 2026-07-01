import { browser } from '$app/environment';

// The dual-language device: one shared choice (Python or Rust) that every
// LangTabs block on every page follows. Rune-based reactive module so any
// component that reads `lang.value` re-renders when the choice changes;
// persisted to localStorage under `wyrd:lang`.

export type Lang = 'python' | 'rust';

const KEY = 'wyrd:lang';

let value = $state<Lang>('python');
let loaded = false;

function read(): Lang | undefined {
  if (!browser) return undefined;
  try {
    const v = localStorage.getItem(KEY);
    return v === 'rust' || v === 'python' ? v : undefined;
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
  { id: 'rust', label: 'Rust' }
];
