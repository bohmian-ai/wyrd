import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import CodeBlock, { copyText } from './CodeBlock.svelte';

const html = '<pre class="shiki"><code><span>mise run dev:backend</span></code></pre>';

describe('CodeBlock', () => {
  const { body } = render(CodeBlock, { props: { html, lang: 'bash', title: 'terminal' } });

  it('labels the sample with its file and language', () => {
    expect(body).toContain('terminal');
    expect(body).toContain('bash');
  });

  it('offers a native, keyboard-operable copy button', () => {
    expect(body).toMatch(/<button[^>]*type="button"[^>]*>/);
    expect(body).toContain('Copy');
  });

  it('announces copy feedback through a polite live region', () => {
    expect(body).toMatch(/aria-live="polite"/);
  });

  it('makes the code a focusable, labeled horizontal scroll region', () => {
    expect(body).toMatch(/<div[^>]*class="code-scroll"[^>]*tabindex="0"/);
    expect(body).toMatch(/class="code-scroll"[^>]*aria-label="terminal code"/);
  });
});

describe('copyText', () => {
  it('reports success when the clipboard accepts the text', async () => {
    let written = '';
    const clipboard = { writeText: async (t: string) => void (written = t) };
    expect(await copyText('mise run dev:backend', clipboard)).toBe('Copied');
    expect(written).toBe('mise run dev:backend');
  });

  it('reports failure instead of staying silent when the clipboard refuses', async () => {
    const clipboard = {
      writeText: async () => {
        throw new Error('denied');
      }
    };
    expect(await copyText('x', clipboard)).toBe('Copy failed');
  });
});
