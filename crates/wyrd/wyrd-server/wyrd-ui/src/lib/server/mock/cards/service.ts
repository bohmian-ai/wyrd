import type {
  ServiceChart,
  ServiceComposition,
  ServiceDefinition,
  ServiceOverviewState,
  ServicePresentation,
  ServiceSignal
} from '$lib/features/cards/core/types';
import type { CardDetailFixture } from './fixtures';

/**
 * TASK-007 — the `checkout-api` Service operational workspace fixture
 * (C-09 plus the S-01…S-07 state package). Every Overview state variant is
 * authored here as a server projection; the browser only selects which
 * variant to render, never computes health. Hrefs are tenant-relative and
 * `{range}` is stamped with the visible range by the workspace.
 *
 * Known mock limitation: the chart series are fixed, so switching the range
 * relabels the same data with different clock windows. Range-varying series
 * are deliberately not generated until a demo needs them.
 */

/** The mock observation clock every chart window ends at. */
const NOW = '2026-09-08T15:42:00Z';
/** Where observations stop in the stale scenario (S-04). */
const STALE_AT = '2026-09-08T15:17:00Z';

/** Shorthand for a canonical Observe link carrying Service and range scope. */
const observe = (signal: string, extra = '') =>
  `/observe/${signal}?service=checkout-api${extra}&range={range}`;

/** The five inline investigations; `note` marks an unauthorized signal (S-05). */
const investigations = (driftNote?: string) => [
  { label: 'Logs', href: observe('logs') },
  { label: 'Metrics', href: observe('metrics') },
  { label: 'Traces', href: observe('traces') },
  { label: 'Evaluations', href: observe('evaluations') },
  driftNote ? { label: 'Drift', note: driftNote } : { label: 'Drift', href: observe('drift') }
];

/** The restrained opt-in Add-chart slot copy shared by the v12 states. */
const addChart = {
  title: '+ ADD CHART',
  lines: [
    'Track another metric for this Service — decline rate, fallback rate, queue depth, token cost. Opt-in, never filler.',
    'Configuration contract: deferred to a specification revision (see README).'
  ]
};

/** The four current operating charts plus the saved custom chart (S-01/C-09). */
function currentCharts(errorLatest: string, errorEnd: number): ServiceChart[] {
  return [
    {
      title: 'REQUESTS',
      measure: 'requests',
      unit: 'req/s',
      latest: '3.4 req/s',
      series: [{ label: 'req/s', points: [3.12, 3.2, 3.16, 3.28, 3.22, 3.31, 3.26, 3.35, 3.3, 3.42, 3.38, 3.4] }],
      min: 3.0,
      source: 'obs 38s · vala metrics',
      link: { label: 'Metrics →', href: observe('metrics') },
      state: 'ok'
    },
    {
      title: 'ERROR RATE',
      measure: 'error rate',
      unit: '%',
      latest: errorLatest,
      series: [{ label: 'errors %', points: [0.14, 0.12, 0.17, 0.15, 0.19, 0.16, 0.21, 0.18, 0.24, 0.22, 0.27, errorEnd] }],
      threshold: { value: 0.5, label: 'budget 0.50%' },
      source: 'obs 38s · vala metrics',
      link: { label: 'Traces →', href: observe('traces') },
      state: 'ok'
    },
    {
      title: 'LATENCY',
      measure: 'latency',
      unit: 'ms · p50 / p95 / p99',
      latest: 'p95 388 ms',
      series: [
        { label: 'p50', points: [214, 218, 211, 222, 219, 226, 221, 228, 224, 231, 227, 229] },
        { label: 'p95', points: [352, 348, 361, 355, 368, 359, 374, 366, 381, 372, 386, 388] },
        { label: 'p99', points: [521, 514, 533, 526, 541, 531, 548, 539, 556, 547, 561, 558] }
      ],
      min: 200,
      threshold: { value: 400, label: 'p95 target 400' },
      source: 'obs 38s · vala traces',
      link: { label: 'Traces →', href: observe('traces') },
      state: 'ok'
    },
    {
      title: 'AVAILABILITY',
      measure: 'availability',
      unit: '%',
      latest: '99.96%',
      series: [{ label: 'availability %', points: [99.95, 99.96, 99.94, 99.96, 99.95, 99.97, 99.96, 99.95, 99.97, 99.96, 99.96, 99.96] }],
      min: 99.9,
      threshold: { value: 99.9, label: 'slo 99.9%' },
      source: 'obs 38s · vala metrics',
      link: { label: 'Metrics →', href: observe('metrics') },
      state: 'ok'
    },
    {
      title: 'CHECKOUT DECLINE RATE',
      measure: 'checkout decline rate',
      unit: '%',
      latest: '2.14%',
      series: [{ label: 'declines %', points: [2.08, 2.11, 2.06, 2.13, 2.09, 2.16, 2.12, 2.18, 2.14, 2.2, 2.16, 2.14] }],
      min: 2.0,
      source: 'custom · saved for this Service',
      link: { label: 'Metrics →', href: observe('metrics') },
      state: 'ok'
    }
  ];
}

/** The published model-drift intelligence panel at a given PSI outcome. */
function driftSignal(breached: boolean, stamp: string): ServiceSignal {
  return {
    title: 'MODEL-DRIFT (Verifier v3)',
    stamp,
    verdict: breached ? { label: '✕ BREACHED', tone: 'danger' } : { label: '✓ WITHIN', tone: 'ok' },
    subject: 'subject ranker (model_primary · Model v12) · feature `cart_value_p50`',
    chart: {
      points: breached
        ? [0.08, 0.09, 0.1, 0.09, 0.11, 0.12, 0.14, 0.13, 0.16, 0.19, 0.23, 0.27]
        : [0.08, 0.09, 0.1, 0.09, 0.11, 0.1, 0.12, 0.11, 0.1, 0.12, 0.11, 0.11],
      threshold: { value: 0.2, label: 'threshold 0.20' },
      axis: ['30d ago', 'today'],
      latest: breached ? 'PSI 0.27' : 'PSI 0.11',
      qualifier: breached ? '> 0.20' : '< 0.20'
    },
    context: 'baseline txns-2026q3 (Data v3) · 48.2k rows · 0.4% missing',
    link: { label: 'Drift analysis →', href: observe('drift', '&driftCard=card_drift_01&feature=cart_value_p50') }
  };
}

/** The published checkout-agent-eval intelligence panel. */
function evalSignal(stamp: string): ServiceSignal {
  return {
    title: 'CHECKOUT-AGENT-EVAL (Verifier v4)',
    stamp,
    verdict: { label: '✓ PASSING', tone: 'ok' },
    subject: 'subject checkout-agent (agent_triage · Agent v6)',
    chart: {
      points: [95.8, 96.1, 95.6, 96.3, 95.9, 96.5, 96.2, 96.7, 96.3, 96.8, 96.5, 96.4],
      min: 95,
      threshold: { value: 95, label: 'pass ≥ 95%' },
      axis: ['10 runs ago', 'latest'],
      latest: '96.4%',
      qualifier: 'pass rate'
    },
    context: '1,240 scenarios per run · deterministic assertions',
    link: { label: 'Evaluations →', href: observe('evaluations') }
  };
}

/** C-09 — the default needs-attention Overview projection. */
const attention: ServiceOverviewState = {
  assessment: {
    tone: 'warn',
    title: '▲ NEEDS ATTENTION',
    sentence: 'Model input drift is over threshold on ranker — checkout ranking quality is at risk.',
    meta: 'server-projected · obs 38s ago · 15:42'
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '● 6/6 · mock projection' },
    { label: 'OPERATIONAL', value: '▲ ATTENTION' },
    { label: 'FRESHNESS', value: '✓ 38s' }
  ],
  channelNote: 'latest change: v12 deployed 09:12 — correlation, not proven cause',
  charts: currentCharts('0.31%', 0.31),
  addChart,
  signals: [driftSignal(true, 'calc 15:30 · 12m ago'), evalSignal('calc 14:41 · 1h ago')],
  attention: {
    title: 'ATTENTION — 1 ACTIVE',
    note: 'server-ordered · degraded first',
    lines: [],
    item: {
      text: '▲ HIGH · 42m — model-drift breach on ranker (model_primary · Model v12)',
      sub: 'feature `cart_value_p50` · PSI 0.27 > threshold 0.20 · calc 12m ago',
      href: '/observe/drift?driftCard=card_drift_01&feature=cart_value_p50&service=checkout-api&range=30d',
      label: '→ /observe/drift · scope preserved'
    }
  },
  context: {
    components: {
      text: 'ranker v12 ▲ · checkout-agent v6 ✓ · fraud-review v2 ✓ · +2 ✓',
      href: 'composition'
    },
    activity: '15:00 drift breach began · 14:41 eval pass 96.4% · 09:12 v12 deploy',
    investigations: investigations(),
    foot: 'scope preserved'
  }
};

/** S-01 — the healthy Overview projection: earned calm, stated from data. */
const healthy: ServiceOverviewState = {
  ...attention,
  assessment: {
    tone: 'ok',
    title: '✓ OPERATING NORMALLY',
    sentence: 'No active attention for v12 in the last hour — all signals current.',
    meta: 'server-projected · obs 41s ago · 15:42'
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '● 6/6 · mock projection' },
    { label: 'OPERATIONAL', value: '✓ NORMAL' },
    { label: 'FRESHNESS', value: '✓ 41s' }
  ],
  charts: currentCharts('0.19%', 0.19),
  signals: [driftSignal(false, 'calc 15:30 · 12m ago'), evalSignal('calc 14:41 · 1h ago')],
  attention: {
    title: 'ATTENTION — NONE ACTIVE',
    note: 'server-ordered · degraded first',
    lines: ['No active attention for v12 in this window — stated from current signal data, not implied by absence.']
  },
  context: {
    ...attention.context,
    components: { text: 'ranker v12 ✓ · checkout-agent v6 ✓ · fraud-review v2 ✓ · +2 ✓', href: 'composition' },
    activity: '15:30 drift calc ✓ · 14:41 eval pass 96.4% · 09:12 v12 deploy'
  }
};

/** S-04 — the stale projection: every chart ends in a labeled gap, never zero. */
const stale: ServiceOverviewState = {
  ...healthy,
  assessment: {
    tone: 'neutral',
    title: '◔ STALE — NO RECENT DATA',
    sentence: 'No observations for v12 in 25 minutes — stale, not healthy; nothing is zeroed or carried forward.',
    meta: 'server-projected · last seen 15:17 · now 15:42'
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '○ UNKNOWN' },
    { label: 'OPERATIONAL', value: '◔ STALE' },
    { label: 'FRESHNESS', value: '▲ 25m OLD' }
  ],
  channelNote: 'latest change: v12 deployed 09:12 — before the gap began',
  charts: currentCharts('', 0.19).map((chart) => ({
    ...chart,
    latest: '— · last 15:17',
    series: chart.series.map((s) => ({ ...s, points: s.points.slice(0, 7) })),
    source: 'obs 15:17 · vala metrics',
    state: 'gap',
    note: 'gap — not zero',
    endsAt: STALE_AT
  })),
  signals: [driftSignal(false, 'calc 14:30 · 1h 12m ago — aged'), evalSignal('calc 14:41 · 1h ago — aged')],
  attention: {
    title: 'ATTENTION — CANNOT EVALUATE',
    note: 'server-ordered · degraded first',
    lines: [
      'No current data to evaluate attention — unknown is not none.',
      'Last known at 15:17: none active.'
    ]
  },
  context: {
    ...healthy.context,
    components: { text: 'all five ◔ stale — no observation since 15:17 · details', href: 'composition' },
    activity: '15:17 last observation · 14:41 eval pass 96.4% · 09:12 v12 deploy'
  }
};

/** S-05 — the partial-authorization projection: drift refused, never zeroed. */
const partial: ServiceOverviewState = {
  ...attention,
  assessment: {
    tone: 'warn',
    title: '◑ PARTIAL — DRIFT UNAUTHORIZED',
    sentence: 'Operational evidence is current; drift summaries require observe:drift scope for j.reyes.',
    meta: 'server-projected · obs 44s ago · 15:42'
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '● 6/6 · mock projection' },
    { label: 'OPERATIONAL', value: '◑ PARTIAL' },
    { label: 'FRESHNESS', value: '✓ 44s' }
  ],
  channelNote: 'unauthorized is never rendered as zero or healthy',
  signals: [
    {
      title: 'MODEL-DRIFT (Verifier v3)',
      stamp: 'publication binding · server-projected',
      state: 'unauthorized',
      body: [
        'requires observe:drift read scope for j.reyes — never shown as zero or healthy',
        'subject stays visible: ranker (model_primary · Model v12) → model-drift',
        'binding from the v12 declaration'
      ]
    },
    evalSignal('calc 14:41 · 1h ago')
  ],
  attention: {
    title: 'ATTENTION — MAY BE INCOMPLETE',
    note: 'server-ordered · degraded first',
    lines: [
      'Drift findings are not visible to you, so this list may be incomplete.',
      'No other active attention in the signals you can read.'
    ]
  },
  context: {
    ...healthy.context,
    investigations: investigations('(scope required)')
  }
};

/** S-06 — the safe backend failure projection: named refusal, local Retry. */
const failed: ServiceOverviewState = {
  ...healthy,
  assessment: {
    tone: 'danger',
    title: '✕ PROJECTION FAILED',
    sentence: 'Operational projection timed out upstream (WYRD_OBS_504_QUERY_TIMEOUT · wy_req_9f2).',
    meta: 'request wy_req_9f2 · 15:42',
    retry: true
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '○ UNKNOWN' },
    { label: 'OPERATIONAL', value: '✕ LOAD FAILED' },
    { label: 'FRESHNESS', value: '— N/A' }
  ],
  channelNote: 'Retry re-requests the operational projection',
  charts: currentCharts('', 0.19).map((chart) => ({
    ...chart,
    latest: undefined,
    series: [],
    state: 'failed',
    note: 'failed with the operational projection · no data returned — not zero',
    code: 'WYRD_OBS_504_QUERY_TIMEOUT'
  })),
  attention: {
    title: 'ATTENTION — NONE ACTIVE',
    note: 'server-ordered · degraded first',
    lines: [
      'No active attention for v12 in this window — stated from current signal data, not implied by absence · from signal sources, unaffected by the failed projection.'
    ]
  }
};

/** S-02 — the deterministic composition lanes shared by desktop and narrow. */
const composition: ServiceComposition = {
  intro: [
    'How this Service is assembled, measured and wired for reaction — 7 linked Cards across deterministic lanes. Select a node to inspect it.',
    'Composition is declaration only: a declared edge never proves a runtime execution or observation occurred. Operational state lives in Overview.'
  ],
  lanes: [
    {
      title: 'INPUTS & DEFINITIONS',
      nodes: [
        { uid: 'card_prompt_02', kind: 'Prompt', name: 'capture-review', version: 'v3', status: '✓ active', note: 'component · 2 aliases (one Card) · prompt of checkout-agent' },
        { uid: 'card_data_01', kind: 'Data', name: 'txns-2026q3', version: 'v3', status: '✓ profiled', note: 'baseline of model-drift' },
        { uid: 'card_policy_01', kind: 'Policy', name: 'checkout-guardrails', version: 'v2', status: '✓ active', note: 'governs this Service' }
      ]
    },
    {
      title: 'RUNTIME COMPOSITION',
      note: 'components of v12',
      nodes: [
        { uid: 'card_agent_01', kind: 'Agent', name: 'checkout-agent', version: 'v6', status: '✓ healthy', note: 'component · agent_triage' },
        { uid: 'card_model_01', kind: 'Model', name: 'ranker', version: 'v12', status: '✓ deployed', note: 'component · model_primary' },
        { uid: 'card_model_02', kind: 'Model', name: 'ranker-shadow', version: 'v11', status: '○ shadow', note: 'component · model_shadow' },
        { uid: 'card_agent_02', kind: 'Agent', name: 'fraud-review', version: 'v2', status: '✓ healthy', note: 'component · agent_inline' },
        { uid: 'card_workflow_02', kind: 'Workflow', name: 'runtime', version: 'v1', status: '✓ active', note: 'component · runtime_workflow' }
      ]
    },
    {
      title: 'MEASUREMENT',
      nodes: [
        {
          uid: 'card_drift_01',
          kind: 'Verifier',
          name: 'model-drift',
          version: 'v3',
          status: '✕ alerting',
          note: 'verified by · on ranker',
          drawer: {
            headline: '✕ alerting — PSI 0.27 > threshold 0.20',
            in: 'in: ranker verified by · via checkout-api v12 · baseline txns-2026q3',
            out: 'out: fires ranking-drift-response (Trigger)',
            observe: {
              label: 'View in Observe →',
              href: '/observe/drift?driftCard=card_drift_01&service=checkout-api&range=30d'
            }
          }
        },
        { uid: 'card_eval_02', kind: 'Verifier', name: 'checkout-agent-eval', version: 'v4', status: '✓ active', note: 'verified by · on checkout-agent' }
      ]
    },
    {
      title: 'REACTION',
      note: 'mock-only continuation',
      nodes: [
        { uid: 'card_trigger_01', kind: 'Trigger', name: 'ranking-drift-response', version: 'v1', status: '✓ active', note: 'fires on breached report' },
        { uid: 'card_operator_01', kind: 'Operator', name: 'investigate-ranking-drift', version: 'v1', status: '✓ active', note: 'invokes · dispatches Workflow runtime' }
      ]
    }
  ],
  edges: [
    { from: 'card_prompt_02', to: 'card_agent_01', label: 'prompt' },
    { from: 'card_agent_01', to: 'card_eval_02', label: 'verified by' },
    { from: 'card_model_01', to: 'card_drift_01', label: 'verified by' },
    { from: 'card_drift_01', to: 'card_trigger_01', label: 'fires' },
    { from: 'card_trigger_01', to: 'card_operator_01', label: 'invokes', route: 'down' },
    { from: 'card_data_01', to: 'card_drift_01', label: 'baseline', route: 'under' },
    { from: 'card_operator_01', to: 'card_workflow_02', label: 'dispatches workflow', route: 'under' }
  ],
  aliasNote:
    'capture_prompt and shared_prompt are two authored aliases resolving to capture-review — one Card, one node. Binding reads as “verified by · through checkout-api v12”, not global ownership. A declared edge never proves a runtime execution or observation occurred.',
  foot: 'Every node keeps its independent uid, version, status and direct Card route — the Service is how you read the system, not a container that owns it.'
};

/** The raw typed wire spec behind the S-03 closed disclosure. */
const specYaml = `apiVersion: wyrd/v1
kind: Service
metadata:
  name: checkout-api
  space: prod
  labels:
    plane: serving
    tier: "1"
spec:
  summary: Customer-facing checkout — capture, hold and decline decisions for every cart.
  entry_point: acme.checkout.app:app
  components:
    - alias: model_primary
      ref: { kind: Model, name: ranker, version: v12 }
      verified_by:
        - verifier: { kind: Verifier, name: model-drift, version: v3 }
          runs_on: { kind: schedule, cron: "0 * * * *" }
    - alias: agent_triage
      ref: { kind: Agent, name: checkout-agent, version: v6 }
      verified_by:
        - verifier: { kind: Verifier, name: checkout-agent-eval, version: v4 }
          runs_on: { kind: observations_ready }
    - alias: agent_inline
      ref: { kind: Agent, name: fraud-review, version: v2 }
    - alias: model_shadow
      ref: { kind: Model, name: ranker-shadow, version: v11 }
    - alias: runtime_workflow
      ref: { kind: Workflow, name: runtime, version: v1 }
    - alias: capture_prompt
      ref: { kind: Prompt, name: capture-review, version: v3 }
    - alias: shared_prompt
      ref: { kind: Prompt, name: capture-review, version: v3 }
  policies:
    - { kind: Policy, name: checkout-guardrails, version: v2 }`;

/** S-03 — the declaration-first Definition presentation for v12. */
const definition: ServiceDefinition = {
  declaration: {
    note: 'immutable · registered 2026-09-01 by j.reyes',
    summary: 'Customer-facing checkout: capture, hold and decline decisions for every cart.',
    entries: [
      { k: 'entry_point', v: 'acme.checkout.app:app — imported by the deploy image, never by Wyrd' },
      { k: 'identity', v: 'derived from card_ref · bound on first deploy contact (no service_account field)' }
    ]
  },
  components: {
    note: 'aliases are runtime names; each ref keeps its own Card',
    rows: [
      { alias: 'model_primary', ref: 'ranker · v12', href: '/cards/card_model_01', kind: 'Model', publishes: 'model-drift (Verifier)', publishesHref: '/cards/card_drift_01' },
      { alias: 'agent_triage', ref: 'checkout-agent · v6', href: '/cards/card_agent_01', kind: 'Agent', publishes: 'checkout-agent-eval (Verifier)', publishesHref: '/cards/card_eval_02' },
      { alias: 'agent_inline', ref: 'fraud-review · v2', href: '/cards/card_agent_02', kind: 'Agent', publishes: '—' },
      { alias: 'model_shadow', ref: 'ranker-shadow · v11', href: '/cards/card_model_02', kind: 'Model', publishes: '—' },
      { alias: 'runtime_workflow', ref: 'runtime · v1', href: '/cards/card_workflow_02', kind: 'Workflow', publishes: '—' },
      { alias: 'capture_prompt', ref: 'capture-review · v3', href: '/cards/card_prompt_02', kind: 'Prompt', publishes: '—' },
      { alias: 'shared_prompt', ref: 'capture-review · v3', href: '/cards/card_prompt_02', kind: 'Prompt', publishes: 'alias of the same Card' }
    ],
    aliasNote: 'capture_prompt and shared_prompt are two authored aliases resolving to one Card — distinct occurrences, one identity.'
  },
  publications: {
    note: 'verified_by bindings resolve to Verifier Cards only',
    component: {
      label: "component-level — components[].verified_by · subject is the component's Card, observed through checkout-api v12",
      flows: [
        {
          from: { label: 'ranker (Model v12)', href: '/cards/card_model_01' },
          to: { label: 'model-drift (Verifier v3)', href: '/cards/card_drift_01' }
        },
        {
          from: { label: 'checkout-agent (Agent v6)', href: '/cards/card_agent_01' },
          to: { label: 'checkout-agent-eval (Verifier v4)', href: '/cards/card_eval_02' }
        }
      ]
    },
    service: {
      label: 'service-level — verified_by · subject is the Service Card itself',
      value: 'NONE DECLARED',
      note: 'v12 declares no Service-subject publication — stated as absent, not hidden.'
    }
  },
  rawSpec: {
    summary: `spec.yaml — ${specYaml.split('\n').length} lines · validated wyrd/v1`,
    meta: 'closed by default · read-only',
    yaml: specYaml,
    note: 'expands inline without navigation · secret values never appear in any expansion'
  },
  principal: {
    note: 'composed governance context',
    entries: [
      { k: 'principal', v: 'svc-checkout-api' },
      { k: 'bound', v: 'on first deploy contact' },
      { k: 'policy', v: 'checkout-guardrails (Policy v2)', href: '/cards/card_policy_01' },
      { k: '', v: 'gate + invoke rules govern this Service' }
    ]
  },
  secrets: 'Credential and secret values are never part of the declaration or this view.'
};

/** S-07 — the historical v11 projection: explicit no-data, never v12 health. */
const v11Overview: ServiceOverviewState = {
  assessment: {
    tone: 'neutral',
    title: '◔ NO OBSERVATIONS FOR v11',
    sentence: 'Historical v11 selected — every projection rescopes; no v12 health is shown.',
    meta: 'server-projected · none in range · 15:42'
  },
  channels: [
    { label: 'CARD STATE', value: '✓ ACTIVE' },
    { label: 'DEPLOYMENT', value: '○ NO REPORT FOR v11' },
    { label: 'OPERATIONAL', value: '◔ NO DATA' },
    { label: 'FRESHNESS', value: '— NONE IN RANGE' }
  ],
  channelNote: 'v11 registered 2026-08-02 · superseded by v12 on 09-01',
  charts: currentCharts('', 0.19).map((chart) => ({
    ...chart,
    latest: undefined,
    series: [],
    state: 'nodata',
    note: 'explicit no-data — never v12 health',
    source: 'v11 · none in range'
  })),
  addChart: {
    title: 'CUSTOM CHARTS',
    lines: ['Saved custom charts follow the selected version scope — none apply to v11.']
  },
  signals: [
    {
      title: 'MODEL-DRIFT — NO v11 CALCULATION',
      state: 'nodata',
      body: ['No drift calculation exists for a v11 publication in this range — stated, never inferred from v12 results.']
    },
    {
      title: 'CHECKOUT-AGENT-EVAL — NO v11 RESULT',
      state: 'nodata',
      body: ['No evaluation result is bound to a v11 publication in this range — stated, never inferred from v12 results.']
    }
  ],
  attention: {
    title: 'ATTENTION — NO v11 DATA',
    note: 'server-ordered · degraded first',
    lines: [
      'No v11 observations or calculations exist in this range.',
      'Absence of data is rendered as unknown, never as calm.'
    ]
  },
  context: {
    components: { text: 'v11 declares its own refs — see Definition', href: 'definition' },
    activity: 'no v11 events in range · v11 registered 2026-08-02',
    investigations: investigations(),
    foot: 'scope preserved'
  }
};

/** The v11 declaration projection: fewer refs, no publication bindings. */
const v11Presentation: ServicePresentation = {
  now: NOW,
  overview: { default: v11Overview, variants: {} },
  composition: {
    intro: [
      'The v11 declaration — 5 linked Cards. Select a node to inspect it.',
      'Composition is declaration only: a declared edge never proves a runtime execution or observation occurred.'
    ],
    lanes: [
      {
        title: 'INPUTS & DEFINITIONS',
        nodes: [
          { uid: 'card_prompt_02', kind: 'Prompt', name: 'capture-review', version: 'v3', status: '✓ active', note: 'component · capture_prompt' }
        ]
      },
      {
        title: 'RUNTIME COMPOSITION',
        note: 'components of v11',
        nodes: [
          { uid: 'card_agent_01', kind: 'Agent', name: 'checkout-agent', version: 'v6', status: '✓ healthy', note: 'component · agent_triage' },
          { uid: 'card_model_01', kind: 'Model', name: 'ranker', version: 'v11', status: '○ superseded', note: 'component · model_primary' },
          { uid: 'card_agent_02', kind: 'Agent', name: 'fraud-review', version: 'v2', status: '✓ healthy', note: 'component · agent_inline' },
          { uid: 'card_workflow_02', kind: 'Workflow', name: 'runtime', version: 'v1', status: '✓ active', note: 'component · runtime_workflow' }
        ]
      },
      { title: 'MEASUREMENT', note: 'v11 declares no publication bindings', nodes: [] },
      { title: 'REACTION', note: 'none declared for v11', nodes: [] }
    ],
    edges: [{ from: 'card_prompt_02', to: 'card_agent_01', label: 'prompt' }],
    aliasNote: 'v11 declares a single capture_prompt alias; the shared_prompt alias arrives in v12.',
    foot: composition.foot
  },
  definition: {
    ...definition,
    declaration: {
      note: 'immutable · registered 2026-08-02 by j.reyes · superseded by v12',
      summary: definition.declaration.summary,
      entries: definition.declaration.entries
    },
    components: {
      note: definition.components.note,
      rows: definition.components.rows
        .filter((row) => !['model_shadow', 'shared_prompt'].includes(row.alias))
        .map((row) => ({
          ...row,
          ref: row.alias === 'model_primary' ? 'ranker · v11' : row.ref,
          publishes: '—',
          publishesHref: undefined
        })),
      aliasNote: 'v11 declares a single capture_prompt alias.'
    },
    publications: {
      note: definition.publications.note,
      component: {
        label: definition.publications.component.label,
        flows: [],
        absent: 'NONE DECLARED — v11 predates the model-drift and checkout-agent-eval bindings.'
      },
      service: {
        label: definition.publications.service.label,
        value: 'NONE DECLARED',
        note: 'v11 declares no Service-subject publication — stated as absent, not hidden.'
      }
    },
    rawSpec: {
      ...definition.rawSpec,
      summary: 'spec.yaml — v11 declaration · validated wyrd/v1',
      yaml: specYaml
        .split('\n')
        .filter((line) => !/model-drift|checkout-agent-eval|verified_by|runs_on|model_shadow|ranker-shadow|shared_prompt/.test(line))
        .join('\n')
        .replace('version: v12 }', 'version: v11 }')
    }
  }
};

/** The complete `checkout-api` detail fixture the mock registry serves. */
export const serviceDetail: CardDetailFixture = {
  uid: 'card_service_01',
  name: 'checkout-api',
  kind: 'Service',
  summary: 'Customer-facing checkout: capture, hold and decline decisions for every cart.',
  status: { label: 'Needs attention', tone: 'warn' },
  version: 'v12',
  currentVersion: 'v12',
  versions: [
    { version: 'v12', created: '2026-09-01 09:02Z', by: 'j.reyes', note: 'bind ranker v12 · publish model-drift' },
    { version: 'v11', created: '2026-08-02 11:15Z', by: 'j.reyes', note: 'selecting v11 rescopes declaration and observations' }
  ],
  actions: [],
  spec: [
    {
      title: 'Spec',
      note: 'presented by the Service workspace',
      entries: [{ k: 'space', v: 'prod' }]
    }
  ],
  presentation: {
    now: NOW,
    overview: { default: attention, variants: { healthy, stale, partial, failed } },
    composition,
    definition
  },
  versionPresentations: { v11: v11Presentation },
  metadata: [
    { k: 'owner', v: 'j.reyes' },
    { k: 'labels', v: 'plane=serving · tier=1' },
    { k: 'created', v: '2026-06-14' },
    { k: 'updated', v: '2026-09-01 (v12)' }
  ],
  relationships: [
    { relation: 'components', label: '7 refs → 6 Cards (aliases)' },
    { relation: 'publication', label: 'model-drift (Verifier)', href: '/cards/card_drift_01' },
    { relation: 'publication', label: 'checkout-agent-eval (Verifier)', href: '/cards/card_eval_02' },
    { relation: 'governed by', label: 'checkout-guardrails (Policy)', href: '/cards/card_policy_01' },
    { relation: 'reaction', label: 'ranking-drift-response (Trigger)', href: '/cards/card_trigger_01' }
  ]
};
