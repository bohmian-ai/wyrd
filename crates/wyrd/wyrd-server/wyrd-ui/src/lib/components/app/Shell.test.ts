import { render, fireEvent, within } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import { createRawSnippet } from 'svelte';
import Shell from './Shell.svelte';

const tenant = { key: 'acme', name: 'Acme' };
const session = {
  subject: { id: 'user', name: 'Jordan Reyes' },
  tenants: [tenant],
  csrf: 'csrf',
  expiresAt: 1
};
const children = createRawSnippet(() => ({ render: () => '<h1>Workspace</h1>' }));

test('exactly five tenant-relative destinations and static single-tenant identity', () => {
  const view = render(Shell, { tenant, session, pathname: '/t/acme/cards/item', children });
  const nav = within(view.getByRole('navigation', { name: 'Primary' }));
  expect(nav.getAllByRole('link').map((link) => link.textContent?.trim())).toEqual([
    'Home',
    'Cards',
    'Observe',
    'Changes',
    'Query'
  ]);
  expect(nav.getByRole('link', { name: 'Cards' })).toHaveAttribute('aria-current', 'page');
  expect(nav.getByRole('link', { name: 'Home' })).toHaveAttribute('href', '/t/acme');
  expect(view.getByLabelText('Current tenant')).toHaveTextContent('Acme');
  expect(view.queryByText('Search tenants')).toBeNull();
  expect(view.getByRole('link', { name: 'Skip to content' })).toHaveAttribute('href', '#main');
});

test('multiple tenants have searchable CSRF-protected switching and accessible mobile navigation', async () => {
  const view = render(Shell, {
    tenant,
    session: { ...session, tenants: [tenant, { key: 'research', name: 'Research' }] },
    pathname: '/t/acme',
    children
  });
  await fireEvent.input(view.getByLabelText('Search tenants'), { target: { value: 'research' } });
  expect(view.queryByRole('button', { name: 'Acme' })).toBeNull();
  const destination = view.getByRole('button', { name: 'Research' });
  expect(destination.closest('form')).toHaveAttribute('action', '/?/switch');
  expect(destination.closest('form')?.querySelector('[name="csrf"]')).toHaveValue('csrf');
  const menu = view.getByRole('button', { name: 'Navigation' });
  await fireEvent.click(menu);
  expect(menu).toHaveAttribute('aria-expanded', 'false');
  await fireEvent.click(menu);
  expect(menu).toHaveAttribute('aria-expanded', 'true');
});
