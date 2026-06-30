import { render, screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Button from './Button.svelte';

test('renders a button with the default primary variant', () => {
  render(Button, { props: { variant: 'primary' } });
  expect(screen.getByRole('button')).toHaveAttribute('data-variant', 'primary');
});

test('applies the rune variant', () => {
  render(Button, { props: { variant: 'rune' } });
  expect(screen.getByRole('button')).toHaveAttribute('data-variant', 'rune');
});

test('forwards native button attributes', () => {
  render(Button, { props: { disabled: true } });
  expect(screen.getByRole('button')).toBeDisabled();
});
