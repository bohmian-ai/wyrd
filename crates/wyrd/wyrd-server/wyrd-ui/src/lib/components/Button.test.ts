import { readFileSync } from 'node:fs';
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { ComponentProps } from 'svelte';
import { expect, test, vi } from 'vitest';
import Button from './Button.svelte';

test('renders a button with the default primary variant', () => {
  render(Button, { props: { variant: 'primary' } });
  expect(screen.getByRole('button')).toHaveAttribute('data-variant', 'primary');
});

test('applies the secondary variant', () => {
  render(Button, { props: { variant: 'secondary' } });
  expect(screen.getByRole('button')).toHaveAttribute('data-variant', 'secondary');
});

test('renders a link when href is given', () => {
  render(Button, { props: { href: '/t/acme/cards' } });
  expect(screen.getByRole('link')).toHaveAttribute('href', '/t/acme/cards');
});

test('exposes exactly the catalog props without native attribute widening', () => {
  const props: Record<keyof ComponentProps<typeof Button>, true> = {
    variant: true, href: true, children: true
  };
  const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8'));
  expect(Object.keys(props).sort()).toEqual(Object.keys(manifest.components.Button.props).sort());
  const source = readFileSync('src/lib/components/Button.svelte', 'utf8');
  expect(source).not.toMatch(/HTML\w+Attributes|\.\.\./);
});

test.each([undefined, '#cards'])('does not forward undeclared capabilities with href=%s', async (href) => {
  const onclick = vi.fn();
  const props = { href, onclick, class: 'injected', style: 'display:none', title: 'undeclared' };
  render(Button, { props });
  const control = screen.getByRole(href ? 'link' : 'button');
  expect(control).not.toHaveClass('injected');
  expect(control).not.toHaveAttribute('style');
  expect(control).not.toHaveAttribute('title');
  await fireEvent.click(control);
  expect(onclick).not.toHaveBeenCalled();
});
