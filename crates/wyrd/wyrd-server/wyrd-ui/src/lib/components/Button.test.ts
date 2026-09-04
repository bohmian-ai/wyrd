import { render, screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
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

test('forwards native button attributes', () => {
  render(Button, { props: { disabled: true } });
  expect(screen.getByRole('button')).toBeDisabled();
});
