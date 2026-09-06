import type { HomeView, Tenant } from '$lib/views';

/** Development fixtures, projected through the same server client as live data. */
export function mockHome(tenant: Tenant): HomeView {
  const base = `/t/${encodeURIComponent(tenant.key)}`;
  if (tenant.key !== 'acme')
    return { attention: [], changes: [], cards: [], summaries: [], recent: [] };
  return {
    attention: [
      {
        title: 'Raise checkout ranking cutoff — PII review failed',
        detail: 'change_01 · CLAIM-3',
        age: '12m',
        href: `${base}/changes/change_01`,
        status: 'Attention',
        tone: 'warn'
      },
      {
        title: 'change_03 · draft missing subjects',
        detail: 'you own this draft',
        age: '1d',
        href: `${base}/changes/change_03`,
        status: 'Draft',
        tone: 'neutral'
      },
      {
        title: 'card_service_05 · fraud-api unowned',
        detail: 'policy / ownership',
        age: '4h',
        href: `${base}/cards/card_service_05`,
        status: 'Action',
        tone: 'warn'
      }
    ],
    changes: [
      {
        id: 'change_01',
        title: 'Raise checkout ranking cutoff',
        owner: 'r.okafor',
        status: 'Open',
        tone: 'running',
        claims: { satisfied: 1, total: 3 },
        age: '12m',
        href: `${base}/changes/change_01`
      },
      {
        id: 'change_02',
        title: 'Retire legacy settlement webhook',
        owner: 'j.reyes',
        status: 'Open',
        tone: 'running',
        claims: { satisfied: 1, total: 1 },
        age: '2h',
        href: `${base}/changes/change_02`
      },
      {
        id: 'change_03',
        title: 'Split ledger write path',
        owner: 'j.reyes',
        status: 'Draft',
        tone: 'neutral',
        claims: { satisfied: 0, total: 2 },
        age: '1d',
        href: `${base}/changes/change_03`
      },
      {
        id: 'change_04',
        title: 'Add groundedness gate to agent',
        owner: 'm.linden',
        status: 'Verified',
        tone: 'ok',
        claims: { satisfied: 3, total: 3 },
        age: '2d',
        href: `${base}/changes/change_04`
      },
      {
        id: 'change_05',
        title: 'Rollback fraud threshold',
        owner: 'r.okafor',
        status: 'Closed',
        tone: 'neutral',
        claims: { satisfied: 1, total: 1 },
        age: '6d',
        href: `${base}/changes/change_05`
      }
    ],
    cards: [
      {
        name: 'checkout-api',
        kind: 'Service',
        version: 'v12',
        space: 'prod',
        status: 'Healthy',
        tone: 'ok',
        seen: '3m',
        href: `${base}/cards/card_service_01`
      },
      {
        name: 'ledger-api',
        kind: 'Service',
        version: 'v4',
        space: 'prod',
        status: 'Healthy',
        tone: 'ok',
        seen: '18m',
        href: `${base}/cards/card_service_02`
      },
      {
        name: 'ranking-api',
        kind: 'Service',
        version: 'v9',
        space: 'prod',
        status: 'Degraded',
        tone: 'warn',
        seen: '26m',
        href: `${base}/cards/card_service_03`
      },
      {
        name: 'checkout-agent',
        kind: 'Service',
        version: 'v6',
        space: 'prod',
        status: 'Healthy',
        tone: 'ok',
        seen: '1h',
        href: `${base}/cards/card_service_04`
      }
    ],
    summaries: [
      {
        label: 'Cards',
        value: 318,
        unit: 'registered',
        detail: '16 kinds · 3 unowned · 12 added this week',
        href: `${base}/cards`
      },
      {
        label: 'Observe',
        value: 1,
        unit: 'signal',
        detail: 'checkout-api error rate 1.2% · ranking drift',
        href: `${base}/observe`,
        signal: true
      }
    ],
    recent: [
      {
        title: 'change_01 / verification',
        detail: 'you left a comment on CLAIM-2',
        href: `${base}/changes/change_01/verification`
      },
      {
        title: 'trace_01',
        detail: 'checkout-api capture failure',
        href: `${base}/observe/traces/trace_01`
      },
      {
        title: 'eval_record_01',
        detail: 'groundedness failed',
        href: `${base}/observe/evaluations/eval_record_01`
      },
      { title: `${base}/query`, detail: 'session history: 3 queries', href: `${base}/query` }
    ]
  };
}
