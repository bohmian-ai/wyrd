import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import DriftPanel from './DriftPanel.svelte';

test('composes a histogram and a trend, and lets status override kind on the top-bar', () => {
  const { container, getByText } = render(DriftPanel, {
    props: {
      feature: 'request_amount',
      dataType: 'NUMERIC',
      drifted: true,
      psi: 0.27,
      threshold: 0.2,
      reference: [4, 10, 22, 30, 20, 10, 4],
      current: [2, 5, 12, 20, 28, 22, 12],
      driftSeries: [0.08, 0.1, 0.12, 0.15, 0.14, 0.19, 0.22, 0.24],
      stats: [
        { label: 'ref mean', value: '142.3' },
        { label: 'cur mean', value: '188.7', alert: true }
      ]
    }
  });
  expect(getByText('drifted')).toBeTruthy();
  expect(container.querySelector('.wy-histo')).toBeTruthy();
  expect(container.querySelector('.wy-trend')).toBeTruthy();
  // drifted → top-bar danger, not the control-bar kind color.
  expect(container.querySelector('.wy-drawer')?.getAttribute('style')).toContain('var(--danger)');
});
