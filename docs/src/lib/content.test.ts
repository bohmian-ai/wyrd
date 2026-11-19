import { describe, it, expect } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { render } from 'svelte/server';
import { toSlug, validateFrontmatter, type DocMetadata } from './content.js';
import DocPage from '../routes/[...slug]/+page.svelte';
import { lastCommitDate, remarkUpdated } from './updated.js';

describe('toSlug', () => {
  it('maps index.md to the directory root (empty string for top-level)', () => {
    expect(toSlug('/src/content/docs/index.md')).toBe('');
  });

  it('maps index.svx to its parent directory slug', () => {
    expect(toSlug('/src/content/docs/start-here/index.svx')).toBe('start-here');
    expect(toSlug('/src/content/docs/cards/index.svx')).toBe('cards');
  });

  it('strips .md and .svx extensions from non-index paths', () => {
    expect(toSlug('/src/content/docs/cards/data.md')).toBe('cards/data');
    expect(toSlug('/src/content/docs/start-here/quickstart.svx')).toBe('start-here/quickstart');
  });

  it('maps nested paths preserving hierarchy', () => {
    expect(toSlug('/src/content/docs/api/errors.md')).toBe('api/errors');
    expect(toSlug('/src/content/docs/guides/build-a-workflow.svx')).toBe(
      'guides/build-a-workflow'
    );
  });

  it('maps local mock pages to the same slugs as published pages', () => {
    expect(toSlug('/mocks/pages/get-started/quickstart.md')).toBe('get-started/quickstart');
    expect(toSlug('/mocks/pages/index.md')).toBe('');
  });
});

describe('validateFrontmatter', () => {
  it('returns typed DocMetadata for a valid frontmatter object', () => {
    const result = validateFrontmatter('/src/content/docs/test.md', {
      title: '  Hello World  ',
      description: 'A test page',
      pillar: 'wyrd',
      section: 'Build and verify',
      topic: 'Declare',
      order: 1,
      draft: false
    });
    expect(result).toEqual({
      title: 'Hello World',
      description: 'A test page',
      pillar: 'wyrd',
      section: 'Build and verify',
      topic: 'Declare',
      order: 1,
      draft: false
    });
  });

  it('throws for a file with no title field, naming the path', () => {
    expect(() => validateFrontmatter('/src/content/docs/missing-title.md', { pillar: 'wyrd' })).toThrow(
      /missing-title\.md/
    );
  });

  it('throws for a file with an empty title, naming the path', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/empty-title.svx', { title: '   ', pillar: 'wyrd' })
    ).toThrow(/empty-title\.svx/);
  });

  it('throws for null frontmatter, naming the path', () => {
    expect(() => validateFrontmatter('/src/content/docs/null-fm.md', null)).toThrow(/null-fm\.md/);
  });

  // pillar is fail-closed: it identifies the owning product surface used by
  // navigation and machine-readable indexes.
  it('throws naming the path when pillar is missing', () => {
    expect(() => validateFrontmatter('/src/content/docs/no-pillar.md', { title: 'X' })).toThrow(
      /no-pillar\.md/
    );
  });

  it('throws when pillar is not one of wyrd, fathom, shared', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/bad-pillar.md', { title: 'X', pillar: 'platform' })
    ).toThrow(/wyrd, fathom, shared/);
  });

  it('accepts the fathom and shared pillars', () => {
    expect(
      validateFrontmatter('/src/content/docs/f.md', { title: 'X', pillar: 'fathom', section: 'Understand' }).pillar
    ).toBe('fathom');
    expect(
      validateFrontmatter('/src/content/docs/s.md', { title: 'X', pillar: 'shared', section: 'Understand' }).pillar
    ).toBe('shared');
  });

  it('omits optional fields not present in raw frontmatter', () => {
    const result = validateFrontmatter('/src/content/docs/minimal.md', {
      title: 'Minimal',
      pillar: 'wyrd',
      section: 'Reference'
    });
    expect(result).toEqual({ title: 'Minimal', pillar: 'wyrd', section: 'Reference' });
  });

  it('drops optional fields of the wrong type instead of forwarding them', () => {
    const result = validateFrontmatter('/src/content/docs/typed.md', {
      title: 'X',
      pillar: 'wyrd',
      section: 'Reference',
      order: '3',
      draft: 'true'
    });
    expect(result).toEqual({ title: 'X', pillar: 'wyrd', section: 'Reference' });
  });

  // Section and topic place a page in the approved map; a typo must fail the
  // build instead of silently moving the page or leaving it unreachable.
  it('throws naming the path when section is missing or unknown', () => {
    expect(() => validateFrontmatter('/src/content/docs/no-section.md', { title: 'X', pillar: 'wyrd' })).toThrow(
      /no-section\.md.*Get started, Build and verify, Connect, Operate, Understand, Reference/
    );
    expect(() =>
      validateFrontmatter('/src/content/docs/old.md', { title: 'X', pillar: 'wyrd', section: 'Run Wyrd' })
    ).toThrow(/old\.md/);
  });

  it('requires a Declare, Observe, or Verify topic under Build and verify', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/no-topic.md', { title: 'X', pillar: 'wyrd', section: 'Build and verify' })
    ).toThrow(/no-topic\.md.*Declare, Observe, Verify/);
  });

  it('rejects a topic in a section without topics', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/deep.md', { title: 'X', pillar: 'wyrd', section: 'Operate', topic: 'Deploy' })
    ).toThrow(/deep\.md.*no topics/);
  });

  it('keeps a YYYY-MM-DD update date and drops any other value', () => {
    const base = { title: 'X', pillar: 'wyrd', section: 'Reference' };
    expect(validateFrontmatter('/src/content/docs/u.md', { ...base, updated: '2026-09-30' }).updated).toBe('2026-09-30');
    expect(validateFrontmatter('/src/content/docs/u.md', { ...base, updated: 'yesterday' }).updated).toBeUndefined();
  });
});

// Update dates come from the repository history of the page source, never the
// build clock, so a page that has not been committed shows no date at all.
describe('page update date', () => {
  const docsRoot = fileURLToPath(new URL('../..', import.meta.url));
  const tracked = `${docsRoot}package.json`;
  const gitDate = (file: string) =>
    execFileSync('git', ['log', '-1', '--format=%cs', '--', file], { cwd: docsRoot, encoding: 'utf8' }).trim();
  const untracked = `${docsRoot}.svelte-kit/updated-probe.md`;
  mkdirSync(`${docsRoot}.svelte-kit`, { recursive: true });
  writeFileSync(untracked, '# probe\n');

  it('is the date of the last commit that changed the file', () => {
    expect(lastCommitDate(tracked)).toBe(gitDate(tracked));
    expect(lastCommitDate(tracked)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it('is absent for a file with no commit history', () => {
    expect(lastCommitDate(untracked)).toBeUndefined();
    expect(lastCommitDate(`${docsRoot}does-not-exist.md`)).toBeUndefined();
  });

  it('replaces any authored value in page frontmatter with the repository date', () => {
    const committed = { filename: tracked, data: { fm: { title: 'X', updated: '2001-01-01' } } };
    remarkUpdated()(undefined, committed);
    expect(committed.data.fm.updated).toBe(gitDate(tracked));

    const uncommitted = { filename: untracked, data: { fm: { title: 'X', updated: '2001-01-01' } } };
    remarkUpdated()(undefined, uncommitted);
    expect(uncommitted.data.fm).toEqual({ title: 'X' });
  });
});

describe('page rendering', () => {
  const show = (metadata: DocMetadata) =>
    render(DocPage, { props: { data: { slug: 'none', metadata, archetype: 'article' } } }).body;

  it('shows the commit date as a machine-readable time', () => {
    expect(show({ title: 'X', pillar: 'wyrd', section: 'Reference', updated: '2026-09-30' })).toContain(
      'Last updated <time datetime="2026-09-30">September 30, 2026</time>'
    );
  });

  it('renders the frontmatter title as the page heading', () => {
    expect(show({ title: 'Declare a Service', pillar: 'wyrd', section: 'Build and verify', topic: 'Declare' })).toMatch(
      /<h1[^>]*>Declare a Service<\/h1>/
    );
  });

  it('shows no date for a page without commit history', () => {
    expect(show({ title: 'X', pillar: 'wyrd', section: 'Reference' })).not.toContain('Last updated');
  });
});
