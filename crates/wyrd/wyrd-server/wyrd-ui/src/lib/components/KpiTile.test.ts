import { render, screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import KpiTile from './KpiTile.svelte';

test('renders label and value', () => {
  render(KpiTile, { props: { label: 'p95 latency', value: '842ms' } });
  expect(screen.getByText('p95 latency')).toBeInTheDocument();
  expect(screen.getByText('842ms')).toBeInTheDocument();
});

test('marks an upward delta with the up trend', () => {
  const { container } = render(KpiTile, {
    props: { label: 'runs', value: 12, delta: '+8%', trend: 'up' }
  });
  expect(container.querySelector('.d')).toHaveAttribute('data-trend', 'up');
});

test('omits the delta region when no delta is given', () => {
  const { container } = render(KpiTile, { props: { label: 'runs', value: 12 } });
  expect(container.querySelector('.d')).toBeNull();
});
