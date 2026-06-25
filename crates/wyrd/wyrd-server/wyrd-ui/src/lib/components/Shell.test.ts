import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import Shell from './Shell.svelte';

const snip = (html: string) => createRawSnippet(() => ({ render: () => html }));

test('renders sidebar, topbar, and main content regions when all snippets are given', () => {
  const { container, getByText } = render(Shell, {
    props: {
      sidebar: snip('<span>nav</span>'),
      topbar: snip('<span>bar</span>'),
      children: snip('<span>main</span>')
    }
  });
  expect(container.querySelector('.shell-side')).toBeTruthy();
  expect(container.querySelector('.shell-top')).toBeTruthy();
  expect(getByText('main')).toBeTruthy();
});

test('omits the sidebar and topbar regions when their snippets are absent', () => {
  const { container } = render(Shell, { props: { children: snip('<span>main</span>') } });
  expect(container.querySelector('.shell-side')).toBeNull();
  expect(container.querySelector('.shell-top')).toBeNull();
  expect(container.querySelector('.shell-content')).toBeTruthy();
});
