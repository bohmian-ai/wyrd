import type { CardDetail, CardRow, SpecSection } from '$lib/features/cards/core/types';
import { serviceDetail } from './service';

/**
 * Development-only Card fixtures for tenant `acme`. Rows cover every
 * registrable kind so the inventory rail, filters and lookup can be proven
 * end to end; uids referenced by Home, Changes and Observe fixtures reuse
 * the same identifiers so cross-feature links resolve.
 */
export const cardRows: CardRow[] = [
  { uid: 'card_service_01', name: 'checkout-api', kind: 'Service', version: 'v12', space: 'prod', status: { label: 'Needs attention', tone: 'warn' }, owner: 'j.reyes', updated: '3m ago', updatedAt: '2026-09-08T14:57:00Z', labels: ['plane=serving'] },
  { uid: 'card_drift_01', name: 'model-drift', kind: 'Drift', version: 'v3', space: 'prod', status: { label: 'Breaching', tone: 'warn' }, owner: 'm.linden', updated: '12m ago', updatedAt: '2026-09-08T14:48:00Z', labels: ['plane=eval'] },
  { uid: 'card_trigger_01', name: 'ranking-drift-response', kind: 'Trigger', version: 'v1', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '12m ago', updatedAt: '2026-09-08T14:48:00Z', labels: ['plane=eval'] },
  { uid: 'card_operator_01', name: 'investigate-ranking-drift', kind: 'Operator', version: 'v1', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '12m ago', updatedAt: '2026-09-08T14:48:00Z', labels: ['plane=eval'] },
  { uid: 'card_service_02', name: 'ledger-api', kind: 'Service', version: 'v4', space: 'prod', status: { label: 'Healthy', tone: 'ok' }, owner: 'j.reyes', updated: '18m ago', updatedAt: '2026-09-08T14:42:00Z', labels: ['plane=serving'] },
  { uid: 'card_service_03', name: 'ranking-api', kind: 'Service', version: 'v9', space: 'prod', status: { label: 'Degraded', tone: 'warn' }, owner: 'm.linden', updated: '26m ago', updatedAt: '2026-09-08T14:34:00Z', labels: ['plane=serving'] },
  { uid: 'card_service_04', name: 'checkout-agent', kind: 'Service', version: 'v6', space: 'prod', status: { label: 'Healthy', tone: 'ok' }, owner: 'r.okafor', updated: '1h ago', updatedAt: '2026-09-08T14:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_agent_01', name: 'checkout-agent', kind: 'Agent', version: 'v6', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'r.okafor', updated: '1h ago', updatedAt: '2026-09-08T13:58:00Z', labels: ['plane=agents'] },
  { uid: 'card_data_01', name: 'txns-2026q3', kind: 'Data', version: 'v3', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '1h ago', updatedAt: '2026-09-08T13:55:00Z', labels: ['plane=data'] },
  { uid: 'card_experiment_01', name: 'checkout-ranking-study', kind: 'Experiment', version: 'v4', space: 'research', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '2h ago', updatedAt: '2026-09-08T13:00:00Z', labels: ['plane=research'] },
  { uid: 'card_generic_01', name: 'settlement-guardrails', kind: 'Policy', version: 'v7', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '2h ago', updatedAt: '2026-09-08T12:58:00Z', labels: ['plane=governance'] },
  { uid: 'card_service_05', name: 'fraud-api', kind: 'Service', version: 'v2', space: 'prod', status: { label: 'Healthy', tone: 'ok' }, owner: '', updated: '2h ago', updatedAt: '2026-09-08T12:55:00Z', labels: ['plane=serving'] },
  { uid: 'card_model_01', name: 'ranker', kind: 'Model', version: 'v12', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '3h ago', updatedAt: '2026-09-08T12:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_prompt_01', name: 'checkout-triage', kind: 'Prompt', version: 'v8', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'r.okafor', updated: '4h ago', updatedAt: '2026-09-08T11:00:00Z', labels: ['plane=agents'] },
  { uid: 'card_service_06', name: 'settlement-api', kind: 'Service', version: 'v7', space: 'prod', status: { label: 'Healthy', tone: 'ok' }, owner: 'j.reyes', updated: '5h ago', updatedAt: '2026-09-08T10:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_artifact_01', name: 'ranker-weights', kind: 'Artifact', version: 'v2', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '1d ago', updatedAt: '2026-09-07T15:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_eval_01', name: 'checkout-quality', kind: 'Eval', version: 'v5', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '1d ago', updatedAt: '2026-09-07T14:00:00Z', labels: ['plane=eval'] },
  { uid: 'card_data_02', name: 'checkout-quality-dataset', kind: 'Data', version: 'v1', space: 'staging', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '2d ago', updatedAt: '2026-09-06T15:00:00Z', labels: ['plane=eval'] },
  { uid: 'card_source_01', name: 'warehouse-telemetry', kind: 'Source', version: 'v2', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '3d ago', updatedAt: '2026-09-05T15:00:00Z', labels: ['plane=data'] },
  { uid: 'card_workflow_01', name: 'checkout-quality', kind: 'Workflow', version: 'v7', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '4d ago', updatedAt: '2026-09-04T15:00:00Z', labels: ['plane=eval'] },
  { uid: 'card_verifier_01', name: 'pii-review', kind: 'Verifier', version: 'v5', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'r.okafor', updated: '5d ago', updatedAt: '2026-09-03T15:00:00Z', labels: ['plane=governance'] },
  { uid: 'card_mcp_01', name: 'payments-mcp', kind: 'Mcp', version: 'v1', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '6d ago', updatedAt: '2026-09-02T15:00:00Z', labels: ['plane=agents'] },
  { uid: 'card_audit_01', name: 'q3-access-audit', kind: 'Audit', version: 'v1', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '2w ago', updatedAt: '2026-08-25T15:00:00Z', labels: ['plane=governance'] },
  // The remaining checkout-api composition refs (TASK-007) — every Service
  // workspace node keeps its own resolvable Card route.
  { uid: 'card_agent_02', name: 'fraud-review', kind: 'Agent', version: 'v2', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'r.okafor', updated: '2d ago', updatedAt: '2026-09-06T12:00:00Z', labels: ['plane=agents'] },
  { uid: 'card_model_02', name: 'ranker-shadow', kind: 'Model', version: 'v11', space: 'prod', status: { label: 'Shadow', tone: 'neutral' }, owner: 'm.linden', updated: '2d ago', updatedAt: '2026-09-06T11:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_workflow_02', name: 'runtime', kind: 'Workflow', version: 'v1', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '3d ago', updatedAt: '2026-09-05T12:00:00Z', labels: ['plane=serving'] },
  { uid: 'card_prompt_02', name: 'capture-review', kind: 'Prompt', version: 'v3', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'r.okafor', updated: '4d ago', updatedAt: '2026-09-04T12:00:00Z', labels: ['plane=agents'] },
  { uid: 'card_policy_01', name: 'checkout-guardrails', kind: 'Policy', version: 'v2', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'j.reyes', updated: '1w ago', updatedAt: '2026-09-01T12:00:00Z', labels: ['plane=governance'] },
  { uid: 'card_eval_02', name: 'checkout-agent-eval', kind: 'Eval', version: 'v4', space: 'prod', status: { label: 'Active', tone: 'ok' }, owner: 'm.linden', updated: '1w ago', updatedAt: '2026-09-01T11:00:00Z', labels: ['plane=eval'] }
];

/** The recently-viewed rail for this principal; presentation-only fixture. */
export const recentCards = [
  { uid: 'card_service_06', name: 'settlement-api', kind: 'Service', viewed: '5m ago' },
  { uid: 'card_data_01', name: 'txns-2026q3', kind: 'Data', viewed: '1h ago' },
  { uid: 'card_model_01', name: 'ranker', kind: 'Model', viewed: '3h ago' }
] satisfies { uid: string; name: string; kind: CardRow['kind']; viewed: string }[];

/**
 * A detail fixture may carry version-scoped Spec projections — and, for a
 * kind with an earned page workspace, version-scoped presentations —
 * selecting a prior version renders its entry, so "read-only at that
 * version" stays true.
 */
export type CardDetailFixture = CardDetail & {
  versionSpecs?: Record<string, SpecSection[]>;
  versionPresentations?: Record<string, CardDetail['presentation']>;
};

/**
 * Full detail fixtures for the presentations this task owns — the shared
 * shell on a Policy (C-02), the earned Workflow (C-08) and Verifier (C-12)
 * presentations — plus interim Model, Data and Service declarations so the
 * primary registry cards answer real questions until their focused
 * workspace tasks land. Every other row gets a generated generic detail.
 * Hrefs are tenant-relative; pages prefix the tenant base. Spec sections
 * carry the declaration only; owner, labels and derived relations render
 * solely in the server-managed rail panels.
 */
export const cardDetails: Record<string, CardDetailFixture> = {
  card_generic_01: {
    uid: 'card_generic_01',
    name: 'settlement-guardrails',
    kind: 'Policy',
    summary: 'Blocks settlement changes that violate PCI isolation, retention or egress rules.',
    status: { label: 'Active', tone: 'ok' },
    version: 'v7',
    currentVersion: 'v7',
    versions: [
      { version: 'v7', created: '2026-08-30 14:02Z', by: 'j.reyes', note: 'add egress allowlist' },
      { version: 'v6', created: '2026-07-12 09:44Z', by: 'j.reyes', note: 'raise retention to 90d' },
      { version: 'v5', created: '2026-06-02 16:10Z', by: 'r.okafor', note: 'initial prod registration' }
    ],
    actions: [{ label: 'Open in Observe', href: '/observe?service=settlement-api' }],
    spec: [
      {
        title: 'Spec',
        note: 'kind-owned sections',
        entries: [
          { k: 'enforcement', v: 'block on violation' },
          { k: 'scope', v: 'space prod' },
          { k: 'rules', v: '3 declared · pci-isolation · retention-90d · egress-allowlist' },
          { k: 'severity', v: 'high' },
          { k: 'review', v: 'quarterly · owner sign-off' },
          { k: 'exceptions', v: 'none active' }
        ]
      }
    ],
    versionSpecs: {
      v6: [
        {
          title: 'Spec',
          note: 'kind-owned sections · as declared at v6',
          entries: [
            { k: 'enforcement', v: 'block on violation' },
            { k: 'scope', v: 'space prod' },
            { k: 'rules', v: '2 declared · pci-isolation · retention-90d' },
            { k: 'severity', v: 'high' },
            { k: 'review', v: 'quarterly · owner sign-off' },
            { k: 'exceptions', v: 'none active' }
          ]
        }
      ],
      v5: [
        {
          title: 'Spec',
          note: 'kind-owned sections · as declared at v5',
          entries: [
            { k: 'enforcement', v: 'block on violation' },
            { k: 'scope', v: 'space prod' },
            { k: 'rules', v: '2 declared · pci-isolation · retention-30d' },
            { k: 'severity', v: 'high' },
            { k: 'review', v: 'quarterly · owner sign-off' },
            { k: 'exceptions', v: 'none active' }
          ]
        }
      ]
    },
    metadata: [
      { k: 'owner', v: 'j.reyes' },
      { k: 'labels', v: 'plane=governance' },
      { k: 'created', v: '2026-06-02' },
      { k: 'updated', v: '2h ago' }
    ],
    relationships: [
      { relation: 'applies_to', label: 'settlement-api (Service)', href: '/cards/card_service_06' },
      { relation: 'subject_of', label: 'change_02', href: '/changes/change_02' }
    ]
  },
  card_workflow_01: {
    uid: 'card_workflow_01',
    name: 'checkout-quality',
    kind: 'Workflow',
    summary: 'Declares the ordered evaluation stages the checkout agent must pass.',
    status: { label: 'Active', tone: 'ok' },
    version: 'v7',
    currentVersion: 'v7',
    versions: [
      { version: 'v7', created: '2026-08-28 11:20Z', by: 'm.linden', note: 'add judge thresholds stage' },
      { version: 'v6', created: '2026-08-10 09:05Z', by: 'm.linden', note: 'initial prod registration' }
    ],
    actions: [{ label: 'Evaluation results ↗', href: '/observe/evaluations?evalCard=card_eval_01' }],
    spec: [],
    presentation: {
      stages: ['retrieve context', 'score tasks', 'judge thresholds', 'publish record'],
      stageNote: 'Stage order is declared here; execution and results belong to Observe.',
      io: [
        { k: 'consumes', v: 'checkout quality dataset (Data)', href: '/cards/card_data_02' },
        { k: 'evaluates', v: 'checkout-agent (Agent)', href: '/cards/card_agent_01' },
        { k: 'produces', v: 'evaluation records → Observe' },
        { k: 'gate', v: 'all required tasks pass' }
      ],
      governance: [{ k: 'governed by', v: 'eval-standards (Policy)' }],
      execution:
        'A Workflow is a declaration — no run results render here. Results live in Observe → Evaluations.'
    },
    metadata: [
      { k: 'owner', v: 'm.linden · data science' },
      { k: 'labels', v: 'plane=eval' },
      { k: 'created', v: '2026-08-10' },
      { k: 'updated', v: '4d ago' }
    ],
    relationships: [
      { relation: 'referenced_by', label: 'checkout quality (Eval)', href: '/cards/card_eval_01' }
    ]
  },
  card_verifier_01: {
    uid: 'card_verifier_01',
    name: 'pii-review',
    kind: 'Verifier',
    summary: 'Checks that a Change introduces no new personal-data egress from checkout.',
    status: { label: 'Active', tone: 'ok' },
    version: 'v5',
    currentVersion: 'v5',
    versions: [
      { version: 'v5', created: '2026-08-22 15:40Z', by: 'r.okafor', note: 'accept DiffEvidence' },
      { version: 'v4', created: '2026-07-30 10:12Z', by: 'r.okafor', note: 'initial prod registration' }
    ],
    actions: [{ label: 'View verification', href: '/changes/change_01/verification' }],
    spec: [],
    presentation: {
      purpose:
        'Reviews the diff and runtime evidence of a Change Request for new personal-data flows out of the checkout plane, and reports findings bound to the exact revision.',
      evidence: [
        { kind: 'TestRunEvidence', required: 'yes', binding: 'sha256 digest · exact revision' },
        { kind: 'DiffEvidence', required: 'yes', binding: 'base → candidate commit pair' }
      ],
      evidenceNote:
        'Evidence bound to an older revision goes stale automatically — it never carries forward.',
      capabilities: [
        { k: 'produces', v: 'VerifierResult · passed | failed | error' },
        { k: 'modes', v: 'Manual · On new evidence' },
        { k: 'billing', v: 'billable per run' },
        { k: 'runtime', v: 'bound per Change via the requirement, not globally' }
      ],
      secrets:
        'No secrets render on a Verifier Card, and its input world is open — accepted kinds are declared, not exhaustive.'
    },
    metadata: [
      { k: 'owner', v: 'r.okafor' },
      { k: 'labels', v: 'plane=governance' },
      { k: 'created', v: '2026-07-30' },
      { k: 'updated', v: '5d ago' }
    ],
    relationships: [
      { relation: 'verifies_for', label: 'change_01 · CLAIM-3', href: '/changes/change_01' },
      { relation: 'governed_by', label: 'verifier-standards (Policy)' }
    ]
  },
  card_model_01: {
    uid: 'card_model_01',
    name: 'ranker',
    kind: 'Model',
    summary: 'Ranking model served by checkout-api to order checkout results.',
    status: { label: 'Active', tone: 'ok' },
    version: 'v12',
    currentVersion: 'v12',
    versions: [
      { version: 'v12', created: '2026-09-05 09:30Z', by: 'm.linden', note: 'retrain on txns-2026q3' },
      { version: 'v11', created: '2026-08-14 10:05Z', by: 'm.linden', note: 'feature pruning' }
    ],
    actions: [{ label: 'Open in Observe', href: '/observe?service=checkout-api' }],
    spec: [
      {
        title: 'Spec',
        note: 'kind-owned sections · interim until the Model workspace lands',
        entries: [
          { k: 'task', v: 'ranking · pairwise' },
          { k: 'interface', v: 'transaction + user context features → score' },
          { k: 'signature', v: 'f32[128] → f32' },
          { k: 'artifact', v: 'ranker-weights v2 (Artifact)', href: '/cards/card_artifact_01' }
        ]
      }
    ],
    metadata: [
      { k: 'owner', v: 'm.linden' },
      { k: 'labels', v: 'plane=serving' },
      { k: 'created', v: '2026-05-11' },
      { k: 'updated', v: '3h ago' }
    ],
    relationships: [
      { relation: 'trained_on', label: 'txns-2026q3 (Data)', href: '/cards/card_data_01' },
      { relation: 'produced_by', label: 'checkout-ranking-study (Experiment)', href: '/cards/card_experiment_01' },
      { relation: 'deployed_by', label: 'checkout-api (Service)', href: '/cards/card_service_01' }
    ]
  },
  card_data_01: {
    uid: 'card_data_01',
    name: 'txns-2026q3',
    kind: 'Data',
    summary: 'Q3 2026 transaction snapshot backing ranker training and drift baselines.',
    status: { label: 'Active', tone: 'ok' },
    version: 'v3',
    currentVersion: 'v3',
    versions: [
      { version: 'v3', created: '2026-09-01 08:00Z', by: 'm.linden', note: 'september refresh' },
      { version: 'v2', created: '2026-08-01 08:00Z', by: 'm.linden', note: 'august refresh' }
    ],
    actions: [{ label: 'Open in Query', href: '/query' }],
    spec: [
      {
        title: 'Spec',
        note: 'kind-owned sections · interim until the Data workspace lands',
        entries: [
          { k: 'format', v: 'iceberg · 42 columns' },
          { k: 'rows', v: '1.2B' },
          { k: 'splits', v: 'train / val / test · 80 / 10 / 10' },
          { k: 'profile', v: 'computed 2026-08-30' }
        ]
      }
    ],
    metadata: [
      { k: 'owner', v: 'm.linden' },
      { k: 'labels', v: 'plane=data' },
      { k: 'created', v: '2026-07-02' },
      { k: 'updated', v: '1h ago' }
    ],
    relationships: [
      { relation: 'used_by', label: 'ranker (Model)', href: '/cards/card_model_01' },
      { relation: 'baseline_for', label: 'model-drift (Drift)', href: '/cards/card_drift_01' }
    ]
  },
  card_service_01: serviceDetail
};
