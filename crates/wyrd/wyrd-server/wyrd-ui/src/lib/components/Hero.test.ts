import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Hero from './Hero.svelte';

test('renders the title, a stat per entry, and a cta per entry', () => {
  const { container, getByText } = render(Hero, {
    props: {
      title: 'The Wyrd Registry',
      tagline: 'Card-bound identity for the agentic stack.',
      stats: [
        { value: '18', label: 'Card kinds' },
        { value: '2', label: 'Planes' }
      ],
      ctas: [
        { label: 'wyrd apply' },
        { label: 'view docs', variant: 'rune' }
      ]
    }
  });
  expect(getByText('The Wyrd Registry')).toBeTruthy();
  expect(container.querySelectorAll('.stat')).toHaveLength(2);
  const ctas = container.querySelectorAll('.cta');
  expect(ctas).toHaveLength(2);
  expect(ctas[1]).toHaveAttribute('data-v', 'rune');
});

test('renders a cta with an href as a link', () => {
  const { container } = render(Hero, {
    props: { title: 'X', ctas: [{ label: 'docs', href: '/docs' }] }
  });
  const cta = container.querySelector('a.cta');
  expect(cta).toHaveAttribute('href', '/docs');
});
