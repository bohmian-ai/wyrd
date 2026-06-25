import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Topbar from './Topbar.svelte';

test('renders breadcrumbs with the last one marked current', () => {
  const { container } = render(Topbar, {
    props: { crumbs: ['registry', 'cards', 'fathom-wf'] }
  });
  const crumbs = container.querySelectorAll('.crumb');
  expect(crumbs).toHaveLength(3);
  expect(crumbs[2].classList.contains('cur')).toBe(true);
  expect(crumbs[0].classList.contains('cur')).toBe(false);
  expect(container.querySelectorAll('.sep')).toHaveLength(2);
});

test('renders a search input and an env pill with a status dot', () => {
  const { container } = render(Topbar, {
    props: { crumbs: ['home'], search: 'search cards…', env: { label: 'prod', status: 'ok' } }
  });
  expect(container.querySelector('input.search')).toHaveAttribute('placeholder', 'search cards…');
  expect(container.querySelector('.env i')).toHaveAttribute('data-st', 'ok');
});
