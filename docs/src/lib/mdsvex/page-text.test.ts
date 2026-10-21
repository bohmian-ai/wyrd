import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { pageText } from './page-text.js';

const page = (slug: string) =>
  readFileSync(new URL(`../../content/docs/${slug}.md`, import.meta.url), 'utf8');

// llms-full.txt carries what a reader of the rendered page sees, as plain
// Markdown: the embedded source itself, not the tag that names it.
describe('pageText', () => {
  it('renders the quickstart with its embedded source bodies and no component markup', () => {
    const text = pageText(page('get-started/quickstart'));
    expect(text).not.toMatch(/^---$/m);
    expect(text).toContain('```python title="quickstart.py"\nfrom wyrd import WyrdState');
    expect(text).toContain('state = WyrdState.declare("support-reply.yaml")');
    expect(text).toContain('let state = WyrdState::declare("support-reply.yaml").await?;');
    expect(text).toContain('const state = await WyrdState.declare("support-reply.yaml");');
    expect(text).toContain('```yaml title="refund-limit.yaml"');
    expect(text).toContain('verifier: ./refund-limit.yaml');
    expect(text).not.toMatch(/<\/?(CodeFromFile|LangTabs?|Tabs?|Aside)\b/);
    expect(text).not.toContain('ANCHOR');
  });

  it('labels tabs and asides and keeps fenced code untouched', () => {
    const text = pageText(
      [
        '<Tabs>',
        '<Tab label="one.yaml">',
        '',
        'body',
        '',
        '</Tab>',
        '</Tabs>',
        '<LangTabs langs={["python"]}>',
        '<LangTab lang="python">',
        '</LangTab>',
        '</LangTabs>',
        '<Aside type="caution" title="Careful">Read this.</Aside>',
        '```html',
        '<Tab label="literal">',
        '```'
      ].join('\n')
    );
    expect(text).toContain('**one.yaml**');
    expect(text).toContain('**Python**');
    expect(text).toContain('**Careful:** Read this.');
    expect(text).toContain('```html\n<Tab label="literal">\n```');
  });

  it('resolves root-absolute links under the deploy base', () => {
    expect(pageText('[Set up](/get-started/setup/) and [anchor](#x) and [out](https://example.com/)')).toBe(
      '[Set up](/wyrd/get-started/setup/) and [anchor](#x) and [out](https://example.com/)'
    );
  });

  it('embeds a selected range and fails on an invalid embed', () => {
    const text = pageText('<CodeFromFile file="examples/quickstart/support-reply.yaml" lang="yaml" range="3-3" />');
    expect(text).toBe('```yaml title="support-reply.yaml"\nkind: Service\n```');
    expect(() => pageText('<CodeFromFile file="examples/missing.yaml" lang="yaml" />')).toThrow(/file not found/);
  });
});
