import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Dist from './Dist.svelte';

test('sizes each segment by its share of the total', () => {
  const { container } = render(Dist, {
    props: {
      segments: [
        { label: 'client', value: 50, color: 'var(--client-bar)' },
        { label: 'server', value: 50, color: 'var(--server-bar)' }
      ]
    }
  });
  const spans = container.querySelectorAll<HTMLElement>('.bar span');
  expect(spans).toHaveLength(2);
  expect(spans[0].style.width).toBe('50%');
});
