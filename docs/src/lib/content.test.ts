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
    expect(toSlug('/src/content/docs/start-here/quickstart.svx')).toBe(
      'start-here/quickstart'
    );
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
      title: 'Hello World',
      description: 'A test page',
      pillar: 'wyrd',
      order: 1,
      draft: false
    });
    expect(result.title).toBe('Hello World');
    expect(result.description).toBe('A test page');
    expect(result.pillar).toBe('wyrd');
    expect(result.order).toBe(1);
    expect(result.draft).toBe(false);
  });

  it('throws for a file with no title field, naming the path', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/missing-title.md', {})
    ).toThrow(/missing-title\.md/);
  });

  it('throws for a file with an empty title, naming the path', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/empty-title.svx', { title: '   ' })
    ).toThrow(/empty-title\.svx/);
  });

  it('throws for null frontmatter, naming the path', () => {
    expect(() =>
      validateFrontmatter('/src/content/docs/null-fm.md', null)
    ).toThrow(/null-fm\.md/);
  });

  it('omits optional fields not present in raw frontmatter', () => {
    const result = validateFrontmatter('/src/content/docs/minimal.md', {
      title: 'Minimal'
    });
    expect(result.title).toBe('Minimal');
    expect(result.description).toBeUndefined();
    expect(result.pillar).toBeUndefined();
    expect(result.group).toBeUndefined();
    expect(result.order).toBeUndefined();
    expect(result.draft).toBeUndefined();
  });
});
