import type { ChangeSummary } from '$lib/features/changes/types';

export const summaries: ChangeSummary[] = [
  {
    id: 'change_01',
    title: 'Raise checkout ranking cutoff',
    owner: 'r.okafor',
    team: 'Engineering',
    service: 'checkout-api',
    lifecycle: 'open',
    verification: { label: 'Needs attention', tone: 'danger' },
    satisfied: 1,
    total: 3,
    repositories: ['acme/checkout', 'acme/ranking'],
    prs: ['4412', '4413', '221'],
    activity: '12m ago',
    attention: [
      { reason: 'Your review requested', actor: 'you — j.reyes, engineering reviewer' }
    ]
  },
  {
    id: 'change_02',
    title: 'Retire legacy settlement webhook',
    owner: 'j.reyes',
    team: 'Engineering',
    service: 'ledger-api',
    lifecycle: 'open',
    verification: { label: 'Verified', tone: 'ok' },
    satisfied: 1,
    total: 1,
    repositories: ['acme/checkout'],
    prs: ['4400'],
    activity: '2h ago',
    attention: [{ reason: 'Changes requested', actor: 'the author — j.reyes (you)' }]
  },
  {
    id: 'change_03',
    title: 'Split ledger write path',
    owner: 'j.reyes',
    team: 'Data Science',
    service: 'ledger-api',
    lifecycle: 'draft',
    verification: { label: 'Not ready', tone: 'neutral' },
    satisfied: 0,
    total: 2,
    repositories: ['acme/ledger'],
    prs: ['51', '52'],
    activity: '1d ago',
    attention: [{ reason: 'Required Evidence missing', actor: 'm.linden — Data Science' }]
  },
  {
    id: 'change_04',
    title: 'Add groundedness gate to agent',
    owner: 'm.linden',
    team: 'Data Science',
    service: 'checkout-agent',
    lifecycle: 'open',
    verification: { label: 'Verified', tone: 'ok' },
    satisfied: 3,
    total: 3,
    repositories: ['acme/agent'],
    prs: ['17'],
    activity: '2d ago',
    attention: []
  },
  {
    id: 'change_05',
    title: 'Rollback fraud threshold',
    owner: 'r.okafor',
    team: 'Product',
    service: 'fraud-api',
    lifecycle: 'closed',
    closure: 'cancelled',
    verification: { label: 'Verified', tone: 'ok' },
    satisfied: 1,
    total: 1,
    repositories: ['acme/fraud'],
    prs: ['8'],
    activity: '6d ago',
    attention: []
  }
];

import type { Change, Subject, Draft, Mention, Check } from '$lib/features/changes/types';
export const mentions: Mention[] = [
  {
    id: '01990000-0000-7000-8000-000000000001',
    name: 'j.reyes',
    team: 'Engineering',
    kind: 'user'
  },
  { id: 'user_product', name: 'r.okafor', team: 'Product', kind: 'user' },
  { id: 'user_ds', name: 'm.linden', team: 'Data Science', kind: 'user' },
  { id: 'team_ds', name: 'data-science', team: 'Data Science', kind: 'team' },
  { id: 'team_eng', name: 'engineering', team: 'Engineering', kind: 'team' }
];
export const subjects: Subject[] = [
  {
    id: 'subject_api',
    repository: 'acme/checkout',
    provider: 'github',
    pr: '4412',
    url: 'https://github.com/acme/checkout/pull/4412',
    base: '9f2c1ad4',
    candidate: '6b81e0c7',
    relationship: 'Stacked base',
    commits: [
      { sha: '9f2c1ad4', title: 'Base — no change' },
      { sha: 'a11e77c2', title: 'Read cutoff from Policy Card' },
      { sha: '4d0b91fa', title: 'Cover high-risk band in tests' },
      { sha: '6b81e0c7', title: 'Candidate — raise cutoff' }
    ],
    files: [
      {
        path: 'src/capture/rank.rs',
        additions: 18,
        deletions: 4,
        lines: [
          { number: 114, kind: ' ', text: 'fn cutoff(band: RiskBand) -> f32 {' },
          { number: 115, kind: '-', text: '    RiskBand::High => 0.50,' },
          { number: 116, kind: '-', text: '    RiskBand::Med => 0.34,' },
          { number: 117, kind: '+', text: '    let cutoff = policy::cutoff_for(band);' },
          { number: 118, kind: '+', text: '    RiskBand::High => cutoff.high,' },
          { number: 119, kind: '+', text: '    RiskBand::Med => cutoff.med,' },
          { number: 120, kind: ' ', text: '    RiskBand::Low => 0.20,' },
          { number: 121, kind: ' ', text: '}' }
        ]
      },
      ...[
        'src/capture/mod.rs',
        'src/policy/cutoff.rs',
        'tests/capture.rs',
        'Cargo.toml',
        'CHANGELOG.md'
      ].map((path, i) => ({
        path,
        additions: [2, 31, 44, 1, 6][i],
        deletions: [0, 0, 6, 1, 0][i],
        lines: [
          {
            number: 1,
            kind: '+' as const,
            text: [
              'mod rank;',
              'pub const HIGH: f32 = 0.62;',
              'assert_eq!(held.high_risk, expected);',
              'version = "0.2.0"',
              'Hold high-risk carts for review.'
            ][i]
          }
        ]
      }))
    ]
  },
  {
    id: 'subject_worker',
    repository: 'acme/checkout',
    provider: 'github',
    pr: '4413',
    url: 'https://github.com/acme/checkout/pull/4413',
    base: '6b81e0c7',
    candidate: '2ad70f19',
    relationship: 'Stacked head — based on subject_api',
    commits: [{ sha: '2ad70f19', title: 'Apply cutoff to worker' }],
    files: [
      {
        path: 'src/worker.rs',
        additions: 2,
        deletions: 0,
        lines: [{ number: 1, kind: '+', text: 'let cutoff = policy::cutoff_for(band);' }]
      }
    ]
  },
  {
    id: 'subject_model',
    repository: 'acme/ranking',
    provider: 'github',
    pr: '221',
    url: 'https://github.com/acme/ranking/pull/221',
    base: 'c40b7e15',
    candidate: '88ff3a02',
    relationship: 'Second repository',
    commits: [{ sha: '88ff3a02', title: 'Update high-risk ranking evaluation' }],
    files: [
      {
        path: 'eval/threshold.yaml',
        additions: 1,
        deletions: 1,
        lines: [
          { number: 1, kind: '-', text: 'threshold: 0.50' },
          { number: 1, kind: '+', text: 'threshold: 0.62' }
        ]
      }
    ]
  }
];
export const newDraft: Draft = {
  title: 'Raise checkout ranking cutoff for high-risk carts',
  intent:
    'High-risk carts are captured too readily — hold borderline carts for review instead of capturing them.',
  impact: 'Held carts rise ~0.6% → ~2.2%; capture errors fall.',
  owner: 'r.okafor',
  teams: 'r.okafor (Product), m.linden (Data Science), j.reyes (Engineering)',
  subjects: subjects.map((subject) => ({
    ...structuredClone(subject),
    candidate: subject.id === 'subject_model' ? '' : subject.candidate
  })),
  claims: [
    {
      id: 'CLAIM-1',
      title: 'Checkout stays correct for high-risk carts',
      checks: [
        {
          name: 'Checkout suite',
          required: true,
          mode: 'on-new-evidence',
          billable: false
        }
      ]
    },
    {
      id: 'CLAIM-2',
      title: 'Ranking quality does not regress',
      checks: [{ name: 'Eval gate', required: true, mode: 'manual', billable: true }]
    },
    {
      id: 'CLAIM-3',
      title: 'No new personal data leaves checkout',
      checks: [{ name: 'PII review', required: true, mode: 'manual', billable: true }]
    }
  ]
};
export function fixtureChange(summary = summaries[0]): Change {
  const primary = summary.id === 'change_01';
  return {
    ...structuredClone(summary),
    title: primary ? 'Raise checkout ranking cutoff for high-risk carts' : summary.title,
    revision: 'rev_07',
    revisionNumber: 7,
    created: '2026-09-03T09:12:00Z',
    author: summary.owner,
    intent: primary
      ? 'High-risk carts are being captured too readily. This raises the ranker cutoff so borderline carts are held for review instead of captured.'
      : summary.title,
    impact: primary
      ? 'Checkout shoppers with risk_band 4–5 (~2% of carts). Held carts rise ~0.6% → ~2.2%; capture errors fall.'
      : 'Review the linked service change and its required Claims.',
    owners: 'r.okafor (Product) · m.linden (Data Science) · j.reyes (Engineering)',
    approval: '1 of 3 approved',
    override: primary ? 'CLAIM-3 · does not verify' : 'None',
    nextAction: primary
      ? 'Blocked — CLAIM-2 test Evidence missing (m.linden) · CLAIM-3 PII review failed (j.reyes) · your review is requested'
      : 'Review the exact revision and its Evidence.',
    subjects: structuredClone(
      primary
        ? subjects
        : summary.prs.map((pr, index) => ({
            ...subjects[0],
            id: `subject_${index + 1}`,
            repository: summary.repositories[0],
            pr,
            url: `https://github.com/${summary.repositories[0]}/pull/${pr}`,
            relationship: '',
            files: [],
            commits: []
          }))
    ),
    claims: primary
      ? [
          {
            id: 'CLAIM-1',
            title: 'Checkout stays correct for high-risk carts',
            resolution: 'satisfied',
            checks: checksFor('CLAIM-1')
          },
          {
            id: 'CLAIM-2',
            title: 'Ranking quality does not regress',
            resolution: 'pending',
            checks: checksFor('CLAIM-2')
          },
          {
            id: 'CLAIM-3',
            title: 'No new personal data leaves checkout',
            resolution: 'not_satisfied',
            checks: checksFor('CLAIM-3')
          }
        ]
      : Array.from({ length: summary.total }, (_, index) => ({
          id: `CLAIM-${index + 1}`,
          title: `${summary.title} — acceptance ${index + 1}`,
          resolution: summary.satisfied > index ? 'satisfied' : 'pending',
          checks:
            summary.lifecycle === 'draft'
              ? []
              : [check(`acceptance-${index + 1}`, 'Acceptance suite', { required: true })]
        })),
    blockers: primary
      ? [
          {
            text: 'CLAIM-2 — ranking test Evidence missing',
            actor: 'm.linden (Data Science)'
          },
          {
            text: 'CLAIM-3 — PII review failed · 2 findings',
            actor: 'j.reyes (Engineering)'
          }
        ]
      : [],
    threads: primary
      ? [
          {
            id: 'thread_claim',
            anchor: { kind: 'claim', revision: 'rev_07', target: 'CLAIM-2' },
            resolved: false,
            transitions: [],
            comments: [
              {
                id: 'comment_claim',
                author: 'user_product',
                replyTo: null,
                revisions: [
                  {
                    id: 'comment_claim_v1',
                    body: 'Can we see the effect on completed orders? @m.linden',
                    editor: 'user_product',
                    at: '2026-09-03T10:00:00Z',
                    predecessor: null,
                    mentions: [mentions[2]]
                  }
                ]
              }
            ]
          },
          {
            id: 'thread_source',
            anchor: {
              kind: 'source',
              revision: 'rev_07',
              target: 'subject_api',
              file: 'src/capture/rank.rs',
              line: 118
            },
            resolved: true,
            transitions: [
              { resolved: false, actor: 'j.reyes', at: '2026-09-03T10:00:00Z' },
              { resolved: true, actor: 'j.reyes', at: '2026-09-03T11:00:00Z' }
            ],
            comments: [
              {
                id: 'comment_source',
                author: mentions[0].id,
                replyTo: null,
                revisions: [
                  {
                    id: 'comment_source_v1',
                    body: 'The cutoff should come from the Policy Card.',
                    editor: mentions[0].id,
                    at: '2026-09-03T09:30:00Z',
                    predecessor: null,
                    mentions: []
                  },
                  {
                    id: 'comment_source_v2',
                    body: 'The cutoff constant should come from the Policy Card, not be inlined here.',
                    editor: mentions[0].id,
                    at: '2026-09-03T10:30:00Z',
                    predecessor: 'comment_source_v1',
                    mentions: []
                  }
                ]
              }
            ]
          },
          {
            id: 'thread_evidence',
            anchor: { kind: 'evidence', revision: 'rev_07', target: 'evidence_checkout' },
            resolved: false,
            transitions: [],
            comments: [
              {
                id: 'comment_evidence',
                author: 'user_ds',
                replyTo: null,
                revisions: [
                  {
                    id: 'comment_evidence_v1',
                    body: 'This case failed on attempt 2 only — flake or a threshold boundary problem?',
                    editor: 'user_ds',
                    at: '2026-09-03T10:52:00Z',
                    predecessor: null,
                    mentions: []
                  }
                ]
              }
            ]
          }
        ]
      : [],
    timeline: primary
      ? [
          {
            id: 'event_override',
            kind: 'decision',
            source: 'Audit',
            at: '2026-09-03T11:40:00Z',
            actor: 'j.reyes',
            text: 'Override authorized for CLAIM-3 — does not verify',
            destination: '#decisions'
          },
          {
            id: 'event_claim',
            kind: 'claim',
            source: 'Audit',
            at: '2026-09-03T11:22:00Z',
            actor: 'system',
            text: 'CLAIM-1 resolution pending → satisfied',
            destination: '/verification#CLAIM-1'
          },
          {
            id: 'event_result',
            kind: 'result',
            source: 'Audit',
            at: '2026-09-03T11:21:00Z',
            actor: 'checkout-verifier',
            text: 'Run completed · verdict passed',
            destination: '/verification#result-checkout'
          },
          {
            id: 'event_run',
            kind: 'run',
            source: 'Audit',
            at: '2026-09-03T11:18:00Z',
            actor: 'system',
            text: 'Run queued on new Evidence',
            destination: '/verification#check-regression'
          },
          {
            id: 'event_evidence',
            kind: 'evidence',
            source: 'Audit',
            at: '2026-09-03T11:17:00Z',
            actor: 'github-actions',
            text: 'CI TestRunEvidence accepted',
            destination: '/verification#evidence-checkout'
          },
          {
            id: 'event_comment',
            kind: 'discussion',
            source: 'Review activity',
            at: '2026-09-03T10:52:00Z',
            actor: 'm.linden',
            text: 'Commented on Evidence test_hold_threshold',
            destination: '/review#thread_evidence'
          },
          {
            id: 'event_commit',
            kind: 'commit',
            source: 'Audit',
            at: '2026-09-03T10:04:00Z',
            actor: 'j.reyes',
            text: 'subject_worker candidate moved → 2ad70f19',
            destination: '/subjects/subject_worker'
          },
          {
            id: 'event_pr',
            kind: 'pr',
            source: 'Audit',
            at: '2026-09-03T09:58:00Z',
            actor: 'github',
            text: 'PR #4413 opened on top of #4412',
            destination: '/subjects/subject_worker'
          },
          {
            id: 'event_rev7',
            kind: 'revision',
            source: 'Audit',
            at: '2026-09-03T09:12:00Z',
            actor: 'system',
            text: 'Revision 7 created with 3 subjects',
            destination: '?revision=rev_07'
          },
          {
            id: 'event_thread',
            kind: 'discussion',
            source: 'Review activity',
            at: '2026-09-02T12:00:00Z',
            actor: 'r.okafor',
            text: 'Opened a thread on CLAIM-2',
            destination: '/review#thread_claim'
          },
          {
            id: 'event_rev6',
            kind: 'revision',
            source: 'Audit',
            at: '2026-09-02T09:12:00Z',
            actor: 'system',
            text: 'Revision 6 created; results went stale',
            destination: '?revision=rev_06'
          }
        ]
      : [],
    priorRevisions: ['rev_06', 'rev_05'],
    ...(summary.lifecycle === 'draft'
      ? { draft: { ...structuredClone(newDraft), title: summary.title } }
      : {})
  };
}

function check(id: string, name: string, patch: Partial<Check> = {}): Check {
  return {
    id,
    name,
    verifier: `${id}-verifier`,
    version: '1',
    required: false,
    mode: 'on-new-evidence',
    billable: false,
    execution: 'completed',
    verdict: 'passed',
    provenance: 'current',
    revision: 'rev_07',
    summary: { label: 'Passed', tone: 'ok' },
    explanation: 'Passed on revision 7 · 11:21Z',
    eligible: false,
    action: null,
    evidence: [
      {
        id: `evidence_${id}`,
        name: 'TestRunEvidence',
        present: true,
        digest: 'sha256:9c02ffa1',
        detail: 'checkout suite · 218 passed, 0 failed · github-actions · subject_api'
      }
    ],
    history: [],
    ...patch
  };
}
export function checksFor(claim: string): Check[] {
  if (claim === 'CLAIM-1')
    return [
      check('checkout', 'Checkout suite', {
        required: true,
        version: '5',
        provenance: 'carried_forward',
        explanation: 'Carried forward from revision 6 — declared inputs unchanged.',
        revision: 'rev_06'
      }),
      check('spotcheck', 'Hold-rate spot check', {
        mode: 'manual',
        billable: true,
        execution: 'not_run',
        verdict: null,
        summary: { label: 'Not run', tone: 'running' },
        eligible: true,
        action: 'Run now',
        explanation: 'Ready — prerequisites present; billable.'
      })
    ];
  if (claim === 'CLAIM-2')
    return [
      check('eval-gate', 'Eval gate', {
        version: '4',
        required: true,
        mode: 'manual',
        execution: 'not_run',
        verdict: null,
        summary: { label: 'Not ready', tone: 'neutral' },
        explanation: 'Ranking test Evidence is missing.',
        evidence: [
          {
            id: 'evidence_ranking',
            name: 'Ranking TestRunEvidence',
            present: false,
            digest: '',
            detail: 'Waiting on m.linden · Data Science'
          }
        ]
      }),
      check('regression', 'Regression suite', {
        version: '3',
        execution: 'running',
        verdict: null,
        summary: { label: 'Verifying', tone: 'running' },
        explanation: 'Started 11:18Z from accepted TestRunEvidence.'
      }),
      check('baseline', 'Baseline comparison', {
        execution: 'queued',
        verdict: null,
        summary: { label: 'Queued', tone: 'warn' },
        explanation: 'Queue position 2 — behind the regression suite.'
      })
    ];
  return [
    check('pii', 'PII review', {
      version: '2',
      required: true,
      mode: 'manual',
      billable: true,
      verdict: 'failed',
      summary: { label: 'Failed', tone: 'danger' },
      explanation:
        '2 findings on revision 7: customer email and address in diagnostic output.',
      eligible: true,
      action: 'Rerun',
      history: [
        { execution: 'cancelled', verdict: null, revision: 'rev_05' },
        { execution: 'timed_out', verdict: null, revision: 'rev_06' },
        { execution: 'errored', verdict: null, revision: 'rev_06' },
        { execution: 'completed', verdict: 'inconclusive', revision: 'rev_06' }
      ]
    }),
    check('secret', 'Secret scan', {
      provenance: 'stale',
      revision: 'rev_06',
      explanation:
        'Last result is bound to revision 6 — stale provenance, verdict unchanged.'
    })
  ];
}

export const verifiers = [
  { name: 'Checkout suite', version: '5', billable: false },
  { name: 'Eval gate', version: '4', billable: true },
  { name: 'PII review', version: '2', billable: true },
  { name: 'Hold-rate spot check', version: '1', billable: true },
  { name: 'Regression suite', version: '3', billable: false },
  { name: 'Baseline comparison', version: '1', billable: false },
  { name: 'Secret scan', version: '1', billable: false }
];
