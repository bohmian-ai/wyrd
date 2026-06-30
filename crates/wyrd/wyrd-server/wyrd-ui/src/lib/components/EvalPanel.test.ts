import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import EvalPanel from './EvalPanel.svelte';

test('renders the score, a metric bar per metric, and flags sub-threshold values', () => {
  const { container, getByText } = render(EvalPanel, {
    props: {
      recordId: 'rec_4a91c7',
      agent: 'fathom-wf',
      score: 0.92,
      pass: true,
      metrics: [
        { label: 'relevance', value: 0.95 },
        { label: 'groundedness', value: 0.79 }
      ],
      threshold: 0.8
    }
  });
  expect(getByText('0.92')).toBeTruthy();
  const metrics = container.querySelectorAll('.d-metric');
  expect(metrics).toHaveLength(2);
  // groundedness (0.79) is below the 0.80 threshold → its value is rendered danger.
  expect(metrics[1].querySelector('.v')?.getAttribute('style')).toContain('var(--danger)');
});

test('a failing eval turns the drawer accent danger', () => {
  const { container } = render(EvalPanel, {
    props: { recordId: 'r', agent: 'a', score: 0.4, pass: false, metrics: [], threshold: 0.8 }
  });
  expect(container.querySelector('.wy-drawer')?.getAttribute('style')).toContain('var(--danger)');
});
