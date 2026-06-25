import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Tree from './Tree.svelte';

test('renders folders as details and leaves with a kind bar', () => {
  const { container } = render(Tree, {
    props: {
      nodes: [
        {
          label: 'cards/',
          open: true,
          children: [
            { label: 'payments-svc', meta: 'Service', kind: 'server' },
            { label: 'fathom-wf', meta: 'Agent', kind: 'client' }
          ]
        }
      ]
    }
  });
  expect(container.querySelectorAll('details')).toHaveLength(1);
  expect(container.querySelector('.leaf[data-k="server"]')).toBeTruthy();
  expect(container.querySelectorAll('.leaf')).toHaveLength(2);
});
