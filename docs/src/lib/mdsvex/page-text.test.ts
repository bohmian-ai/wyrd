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
    expect(text).toContain('```python title="deploy.py"');
    expect(text).toContain('cards.declare_from_path(');
    expect(text).toContain('kind: Service');
    expect(text).not.toMatch(/<\/?(CodeFromFile|LangTabs?|Tabs?|Aside)\b/);
    expect(text).not.toContain('ANCHOR');
  });

  it('shows every language connecting at startup and draining at exit', () => {
    for (const slug of ['get-started/quickstart', 'build-and-verify/observe/record-a-run-and-observations']) {
      const text = pageText(page(slug));
      for (const call of [
        'self.state.start_bifrost()',
        'self.state.start_telemetry()',
        'app.state.desk.state.shutdown()',
        'state.start_bifrost().await?',
        'start_telemetry(&self.state)?',
        'telemetry.shutdown()?;\nstate.shutdown().await?;',
        'await state.startBifrost()',
        'await state.startTelemetry()',
        'await state.shutdown()'
      ]) {
        expect(text, `${slug}: ${call}`).toContain(call);
      }
    }
  });

  it('continues the record-a-run path with the declared support-desk Service', () => {
    const text = pageText(page('build-and-verify/observe/record-a-run-and-observations'));
    const card = (name: string) =>
      readFileSync(new URL(`../../../../examples/support-desk-app/service/${name}`, import.meta.url), 'utf8');
    const service = card('support-desk.yaml');
    expect(text).toMatch(/wyrd pull --space default --kind Service --name support-desk --version 1\.0\.0 \\\n\s+--output-dir \S+/);
    expect(text).not.toMatch(/wyrd get[^\n`]*--output-dir/);
    for (const [, table] of service.matchAll(/^    - name: (\w+)$/gm)) {
      expect(text).toContain(`vala.datasets.${table}`);
    }
    const bound = [...service.matchAll(/verifier: \.\/([\w-]+\.yaml)/g)].map((m) => m[1]);
    expect(bound).toHaveLength(3);
    for (const file of bound) {
      const verifier = card(file);
      const name = verifier.match(/^  name: ([\w-]+)$/m)![1];
      const slots = verifier.match(/^  inputs:\n((?: {4}\w+:.*\n)+)/m)![1].matchAll(/^ {4}(\w+):/gm);
      expect(text).toContain(`"${name}"`);
      for (const [, slot] of slots) expect(text, `${name} slot ${slot}`).toMatch(new RegExp(`\\b${slot}\\b`));
    }
    expect(text).not.toMatch(/vala\.datasets\.answers|my-check|my-review|report\.pdf/);
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
    const text = pageText('<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" range="2-2" />');
    expect(text).toBe('```yaml title="support-desk.yaml"\nkind: Service\n```');
    expect(() => pageText('<CodeFromFile file="examples/missing.yaml" lang="yaml" />')).toThrow(/file not found/);
  });
});
