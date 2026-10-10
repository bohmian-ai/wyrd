import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import { lang } from '$lib/lang.svelte';
import LangTabs from './LangTabs.svelte';

const empty = createRawSnippet(() => ({ render: () => '<span></span>' }));

function strip(langs: ('python' | 'rust' | 'typescript')[]): string {
  return render(LangTabs, { props: { langs, children: empty } }).body;
}

describe('LangTabs', () => {
  afterEach(() => lang.set('python'));

  it('offers exactly the languages the block has panels for', () => {
    const body = strip(['python', 'rust', 'typescript']);
    expect(body).toContain('>TypeScript<');
    expect(strip(['python'])).not.toContain('>Rust<');
  });

  it('selects the shared choice when the block has it', () => {
    lang.set('typescript');
    expect(strip(['python', 'rust', 'typescript'])).toMatch(
      /aria-selected="true"[^>]*>\s*TypeScript/
    );
  });

  it('falls back to the first panel when the block lacks the shared choice', () => {
    lang.set('typescript');
    expect(strip(['python', 'rust'])).toMatch(/aria-selected="true"[^>]*>\s*Python/);
  });
});
