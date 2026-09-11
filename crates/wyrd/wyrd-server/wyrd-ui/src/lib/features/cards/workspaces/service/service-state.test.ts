import { expect, test } from 'vitest';
import {
  chartWindow,
  observeHref,
  readServiceScope,
  serviceHref
} from './service-state';

// TASK-007 scenario 1 — the Service workspace's URL state is the only state:
// view/version/range restore from a pasted URL, unknown values collapse to
// truthful defaults, and every link preserves the scope it claims to.

const params = (search: string) => new URLSearchParams(search);

test('view, range, mock state and selection restore from the URL', () => {
  expect(readServiceScope(params('view=composition&range=6h&sel=card_drift_01'))).toEqual({
    view: 'composition',
    range: '6h',
    state: '',
    sel: 'card_drift_01'
  });
  expect(readServiceScope(params('view=overview&state=stale'))).toEqual({
    view: 'overview',
    range: '1h',
    state: 'stale',
    sel: ''
  });
});

test('unknown view, range or mock state collapse to defaults, never invented', () => {
  expect(readServiceScope(params('view=alerts&range=90d&state=exploded'))).toEqual({
    view: 'overview',
    range: '1h',
    state: '',
    sel: ''
  });
});

test('serviceHref rewrites one key and keeps the rest of the scope', () => {
  const scope = { view: 'overview', range: '6h', state: '', sel: '' } as const;
  expect(serviceHref('/t/acme/cards/card_service_01', 'v12', scope, {})).toBe(
    '/t/acme/cards/card_service_01?view=overview&version=v12&range=6h'
  );
  // Range belongs to the operational view only; composition drops it.
  expect(serviceHref('/t/acme/cards/card_service_01', 'v12', scope, { view: 'composition' })).toBe(
    '/t/acme/cards/card_service_01?view=composition&version=v12'
  );
  // Selecting a node keeps the composition scope; a mock state survives a range change.
  expect(
    serviceHref('/t/acme/cards/card_service_01', 'v12', { ...scope, view: 'composition' }, { sel: 'card_drift_01' })
  ).toBe('/t/acme/cards/card_service_01?view=composition&version=v12&sel=card_drift_01');
  expect(
    serviceHref('/t/acme/cards/card_service_01', 'v12', { ...scope, state: 'failed' }, { range: '1h' })
  ).toBe('/t/acme/cards/card_service_01?view=overview&version=v12&range=1h&state=failed');
  // Retry clears only the mock failure state.
  expect(
    serviceHref('/t/acme/cards/card_service_01', 'v12', { ...scope, state: 'failed' }, { state: '' })
  ).toBe('/t/acme/cards/card_service_01?view=overview&version=v12&range=6h');
  // Version selection rescopes in place.
  expect(serviceHref('/t/acme/cards/card_service_01', 'v11', scope, {})).toBe(
    '/t/acme/cards/card_service_01?view=overview&version=v11&range=6h'
  );
});

test('observeHref stamps the visible range into a canonical Observe link', () => {
  expect(observeHref('/observe/metrics?service=checkout-api&range={range}', '6h')).toBe(
    '/observe/metrics?service=checkout-api&range=6h'
  );
});

test('chartWindow derives UTC axis stamps from the shared range and window end', () => {
  const window = chartWindow('1h', '2026-09-08T15:42:00Z');
  expect(window.from.label).toBe('14:42');
  expect(window.mid.label).toBe('15:12');
  expect(window.to.label).toBe('15:41');
  expect(window.from.at).toBe('2026-09-08T14:42:00.000Z');
  // A stale window truthfully ends where observations stop.
  const stale = chartWindow('1h', '2026-09-08T15:42:00Z', '2026-09-08T15:17:00Z');
  expect(stale.to.label).toBe('15:17');
});
