import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import CodeBlock from './CodeBlock.svelte';

test('renders the code verbatim', () => {
  const { container } = render(CodeBlock, { props: { code: 'const x = 1;' } });
  expect(container.querySelector('pre code')?.textContent).toBe('const x = 1;');
});

test('omits the copy button when disabled', () => {
  const { container } = render(CodeBlock, { props: { code: 'x', copy: false } });
  expect(container.querySelector('.copy')).toBeNull();
});
