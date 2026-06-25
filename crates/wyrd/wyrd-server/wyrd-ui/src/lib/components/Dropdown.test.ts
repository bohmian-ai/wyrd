import { fireEvent, render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Dropdown from './Dropdown.svelte';

test('opens the menu and reports the chosen value', async () => {
  let picked = '';
  const { container, getByText } = render(Dropdown, {
    props: {
      label: 'kind: All',
      options: [{ label: 'All' }, { label: 'Agent', kind: 'client' }],
      onselect: (v: string) => (picked = v)
    }
  });
  expect(container.querySelector('.menu')).toBeNull();

  await fireEvent.click(getByText('kind: All'));
  expect(container.querySelector('.menu')).toBeTruthy();

  await fireEvent.click(getByText('Agent'));
  expect(picked).toBe('Agent');
  expect(container.querySelector('.menu')).toBeNull();
});
