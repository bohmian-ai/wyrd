import { render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { expect, test } from 'vitest';
import ChartPanel from './ChartPanel.svelte';

const plot = createRawSnippet(() => ({ render: () => '<svg data-testid="plot"></svg>' }));

const ok = {
  title: 'Request latency',
  measure: 'Latency percentiles',
  unit: 'ms',
  latestValue: 402,
  source: 'vala · checkout-api',
  freshness: { label: '2m ago', at: '2026-09-04T17:18:00Z' },
  from: { label: 'Sep 3 17:20', at: '2026-09-03T17:20:00Z' },
  to: { label: 'Sep 4 17:20', at: '2026-09-04T17:20:00Z' },
  link: { label: 'Open in Observe', href: '/t/acme/observe/metrics' },
  children: plot
};

test('states what is measured, in what unit, how fresh, over which range, and from where', () => {
  const { container } = render(ChartPanel, { props: ok });
  const text = container.textContent ?? '';
  expect(text).toContain('Latency percentiles');
  expect(text).toContain('ms');
  expect(text).toContain('402');
  expect(text).toContain('2m ago');
  expect(text).toContain('vala · checkout-api');
  expect(screen.getByRole('link', { name: 'Open in Observe' })).toHaveAttribute(
    'href',
    '/t/acme/observe/metrics'
  );
  expect(screen.getByTestId('plot')).toBeInTheDocument();
});

test('freshness and range are machine-readable instants, not just formatted prose', () => {
  const { container } = render(ChartPanel, { props: ok });
  const stamps = [...container.querySelectorAll('time')].map((t) => t.getAttribute('datetime'));
  expect(stamps).toEqual([
    '2026-09-04T17:18:00Z',
    '2026-09-03T17:20:00Z',
    '2026-09-04T17:20:00Z'
  ]);
});

test.each(['loading', 'empty', 'unauthorized', 'error'] as const)(
  'the %s state replaces the plot and never shows a latest value',
  (state) => {
    const { container } = render(ChartPanel, {
      props: { ...ok, state, detail: 'server said so', code: 'WYRD-QUERY-TIMEOUT' }
    });
    // No plot, and above all no number: a reading the server did not supply must never be
    // rendered as one, because a stale or absent value shown as current is a health claim.
    expect(screen.queryByTestId('plot')).toBeNull();
    expect(container.querySelector('.latest')).toBeNull();
    expect(container.querySelector('.wy-state')).toHaveAttribute('data-state', state);
    expect(container.textContent).toContain('Latency percentiles');
  }
);

test('the loading state is announced rather than shown as an empty frame', () => {
  render(ChartPanel, { props: { ...ok, state: 'loading' } });
  expect(screen.getByRole('status')).toHaveAttribute('aria-busy', 'true');
});

test('an error state carries the stable Wyrd code and the safe cause', () => {
  const { container } = render(ChartPanel, {
    props: {
      ...ok,
      state: 'error',
      detail: 'The metric query exceeded its time budget.',
      code: 'WYRD-QUERY-TIMEOUT'
    }
  });
  const text = container.textContent ?? '';
  expect(text).toContain('WYRD-QUERY-TIMEOUT');
  expect(text).toContain('The metric query exceeded its time budget.');
});

test('a partial series is drawn and labeled partial at the same time', () => {
  const { container } = render(ChartPanel, {
    props: { ...ok, state: 'partial', detail: '200 of 412 points loaded.' }
  });
  // Partial is the one unavailable state that still has something true to plot; it must
  // not silently present the incomplete series as complete.
  expect(screen.getByTestId('plot')).toBeInTheDocument();
  expect(container.querySelector('.wy-state')).toHaveAttribute('data-state', 'partial');
  expect(container.textContent).toContain('200 of 412 points loaded.');
  expect(container.querySelector('.latest')).toBeNull();
});
