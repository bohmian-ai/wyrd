import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import * as development from '$lib/server/development';
import TenantLayout from '../routes/t/[tenantKey]/+layout.svelte';
import SignedIn from '../routes/t/[tenantKey]/+page.svelte';

const tenant = { key: 'acme', name: 'Acme' };
const session = {
  subject: { id: 'user', name: 'Jordan Reyes' },
  tenants: [tenant, { key: 'research', name: 'Research' }],
  expiresAt: 1
};

test('TASK-002: Retained pages without mocks', () => {
  // Retained routes render plain markup: no deleted shared component or feature imports.
  const sources = import.meta.glob('/src/routes/**/*.svelte', {
    query: '?raw',
    import: 'default',
    eager: true
  }) as Record<string, string>;
  for (const [path, source] of Object.entries(sources))
    expect(source, path).not.toMatch(/\$lib\/(components|features)\//);

  // The mock-data switch is gone; only the development identity remains.
  expect(Object.keys(development).filter((name) => /mock/i.test(name))).toEqual([]);

  // The signed-in page shows tenant and principal with no domain data.
  const page = render(SignedIn, { data: { tenant, session } } as never);
  expect(page.getByRole('heading', { level: 1 })).toHaveTextContent('Acme');
  expect(page.container).toHaveTextContent('Jordan Reyes');
  expect(page.container.textContent).not.toMatch(/Recent changes|Recently viewed Cards/);
  page.unmount();

  // The tenant layout keeps tenant switching and sign-out for the current tenant.
  const children = createRawSnippet(() => ({ render: () => '<p>child</p>' }));
  const layout = render(TenantLayout, { data: { tenant, session }, children } as never);
  const switches = layout.container.querySelectorAll('form[action="/?/switch"] button[name="tenantKey"]');
  expect([...switches].map((button) => button.getAttribute('value'))).toEqual(['acme', 'research']);
  const signOut = layout.getByRole('button', { name: 'Sign out' });
  expect(signOut.closest('form')).toHaveAttribute('action', '/?/logout');
  expect(signOut.closest('form')?.querySelector('input[name="tenantKey"]')).toHaveValue('acme');
  expect(layout.getByText('child')).toBeInTheDocument();
});
