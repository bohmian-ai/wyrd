import { describe, it, expect } from 'vitest';
import { toSlug, validateFrontmatter } from './content.js';

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
});

describe('validateFrontmatter', () => {
  it('returns typed DocMetadata for a valid frontmatter object', () => {
    const result = validateFrontmatter('/src/content/docs/test.md', {
      title: '  Hello World  ',
      description: 'A test page',
      pillar: 'wyrd',
      section: 'Learn Wyrd',
      group: 'Concepts',
      order: 1,
      draft: false
    });
    expect(result).toEqual({
      title: 'Hello World',
      description: 'A test page',
      pillar: 'wyrd',
      section: 'Learn Wyrd',
      group: 'Concepts',
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
      validateFrontmatter('/src/content/docs/f.md', { title: 'X', pillar: 'fathom' }).pillar
    ).toBe('fathom');
    expect(
      validateFrontmatter('/src/content/docs/s.md', { title: 'X', pillar: 'shared' }).pillar
    ).toBe('shared');
  });

  it('omits optional fields not present in raw frontmatter', () => {
    const result = validateFrontmatter('/src/content/docs/minimal.md', {
      title: 'Minimal',
      pillar: 'wyrd'
    });
    expect(result).toEqual({ title: 'Minimal', pillar: 'wyrd' });
  });

  it('drops optional fields of the wrong type instead of forwarding them', () => {
    const result = validateFrontmatter('/src/content/docs/typed.md', {
      title: 'X',
      pillar: 'wyrd',
      order: '3',
      draft: 'true'
    });
    expect(result).toEqual({ title: 'X', pillar: 'wyrd' });
  });
});
