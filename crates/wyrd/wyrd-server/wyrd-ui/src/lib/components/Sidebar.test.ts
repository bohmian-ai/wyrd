import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Sidebar from './Sidebar.svelte';

test('renders grouped nav items and marks the active one', () => {
  const { container, getByText } = render(Sidebar, {
    props: {
      brand: 'wyrd',
      groups: [
        {
          label: 'registry',
          items: [
            { label: 'cards', href: '/cards', active: true },
            { label: 'evals', href: '/evals' }
          ]
        }
      ]
    }
  });
  expect(getByText('wyrd')).toBeTruthy();
  const items = container.querySelectorAll('.item');
  expect(items).toHaveLength(2);
  expect(items[0].classList.contains('active')).toBe(true);
  expect(items[1].classList.contains('active')).toBe(false);
});
