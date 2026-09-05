/** Temporary UI projections; the Wyrd client supplies meaning and action eligibility. */
export type Tone = 'neutral' | 'ok' | 'warn' | 'danger' | 'running';
export type Status = { label: string; tone: Tone };
export type ChangeSummary = {
  id: string;
  title: string;
  owner: string;
  team: string;
  service: string;
  lifecycle: 'draft' | 'open' | 'closed';
  closure?: 'completed' | 'cancelled';
  verification: Status;
  satisfied: number;
  total: number;
  repositories: string[];
  prs: string[];
  activity: string;
  attention: { reason: string; actor: string }[];
};
export type ChangeList = { records: ChangeSummary[]; counts: Record<string, number> };
export type Subject = {
  id: string;
  repository: string;
  provider: string;
  pr: string;
  url: string;
  base: string;
  candidate: string;
  relationship: string;
  commits: { sha: string; title: string }[];
  files: {
    path: string;
    additions: number;
    deletions: number;
    /** Context rows use the new side; deletions use old and additions use new. */
    lines: { number: number; side: 'old' | 'new'; kind: ' ' | '+' | '-'; text: string }[];
  }[];
};
export type Check = {
  id: string;
  name: string;
  verifier: string;
  version: string;
  required: boolean;
  mode: 'manual' | 'on-new-evidence';
  billable: boolean;
  execution:
    | 'not_run'
    | 'queued'
    | 'running'
    | 'completed'
    | 'cancelled'
    | 'timed_out'
    | 'errored';
  verdict: 'passed' | 'failed' | 'inconclusive' | null;
  provenance: 'current' | 'stale' | 'carried_forward';
  revision: string;
  summary: Status;
  explanation: string;
  eligible: boolean;
  action: 'Run now' | 'Rerun' | null;
  evidence: {
    id: string;
    name: string;
    present: boolean;
    digest: string;
    detail: string;
  }[];
  history: { execution: string; verdict: string | null; revision: string }[];
};
export type Claim = {
  id: string;
  title: string;
  resolution: 'pending' | 'satisfied' | 'not_satisfied';
  checks: Check[];
};
export type Anchor = { revision: string; target: string } & (
  | { kind: 'source'; file: string; line: number; side: 'old' | 'new' }
  | {
      kind: 'change' | 'claim' | 'subject' | 'evidence' | 'result';
      file?: never;
      line?: never;
      side?: never;
    }
);
export type Mention = { id: string; name: string; team: string; kind: 'user' | 'team' };
export type CommentRevision = {
  id: string;
  body: string;
  editor: string;
  at: string;
  predecessor: string | null;
  mentions: Mention[];
};
export type Comment = {
  id: string;
  author: string;
  revisions: CommentRevision[];
  replyTo: string | null;
};
export type Thread = {
  id: string;
  anchor: Anchor;
  resolved: boolean;
  comments: Comment[];
  transitions: { resolved: boolean; actor: string; at: string }[];
};
export type TimelineEvent = {
  id: string;
  kind: string;
  source: 'Audit' | 'Review activity';
  at: string;
  actor: string;
  text: string;
  destination: string;
};
export type Draft = {
  title: string;
  intent: string;
  impact: string;
  owner: string;
  teams: string;
  subjects: Subject[];
  claims: {
    id: string;
    title: string;
    checks: {
      name: string;
      required: boolean;
      mode: 'manual' | 'on-new-evidence';
      billable: boolean;
    }[];
  }[];
};
export type Change = ChangeSummary & {
  revision: string;
  revisionNumber: number;
  created: string;
  author: string;
  intent: string;
  impact: string;
  owners: string;
  approval: string;
  override: string;
  nextAction: string;
  subjects: Subject[];
  claims: Claim[];
  blockers: { text: string; actor: string }[];
  threads: Thread[];
  timeline: TimelineEvent[];
  priorRevisions: string[];
  draft?: Draft;
};
export type VerifierChoice = { name: string; version: string; billable: boolean };
export type ChangeView = {
  verifiers: VerifierChoice[];
  change: Change;
  capabilities: { write: boolean; review: boolean; run: boolean; override: boolean };
  mentions: Mention[];
  requestKey: string;
};
export type ActionResult = {
  threadId?: string;
  commentId?: string;
  message?: string;
  problem?: import('$lib/views').WyrdProblem;
  body?: string;
  currentCommentRevision?: string;
  currentRevision?: string;
  draft?: Draft;
};
export type ReviewInput = {
  operation:
    | 'comment'
    | 'reply'
    | 'edit'
    | 'resolve'
    | 'reopen'
    | 'decision'
    | 'close'
    | 'override';
  revision: string;
  requestKey: string;
  body: string;
  threadId: string;
  commentId: string;
  expected: string;
  anchor?: Anchor;
  decision: string;
};
