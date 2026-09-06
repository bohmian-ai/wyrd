import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Topbar from './Topbar.svelte';

test('renders breadcrumbs with the last one marked current', () => {
  const { container } = render(Topbar, {
    props: { crumbs: ['registry', 'cards', 'checkout-wf'] }
  });
  const crumbs = container.querySelectorAll('.crumb');
  expect(crumbs).toHaveLength(3);
  expect(crumbs[2].classList.contains('cur')).toBe(true);
  expect(crumbs[0].classList.contains('cur')).toBe(false);
  expect(container.querySelectorAll('.sep')).toHaveLength(2);
});

test('renders a search input and an env badge that states its status', () => {
  const { container } = render(Topbar, {
    props: { crumbs: ['home'], search: 'search cards…', env: { label: 'prod', status: 'warn' } }
  });
  expect(container.querySelector('input.search')).toHaveAttribute('placeholder', 'search cards…');
  const badge = container.querySelector('.wy-badge');
  expect(badge).toHaveAttribute('data-tone', 'warn');
  expect(badge?.textContent).toContain('prod');
});
