/**
 * Card feature view models. These are BFF projections of server truth for
 * presentation only — the server owns identity, relationships and status
 * (INV-001/INV-009); nothing here becomes a durable contract (INV-014).
 */

/** Every registrable Card kind the inventory can filter by, in rail order. */
export const CARD_KINDS = [
  'Service',
  'Data',
  'Model',
  'Artifact',
  'Experiment',
  'Prompt',
  'Agent',
  'Workflow',
  'Mcp',
  'Policy',
  'Audit',
  'Drift',
  'Eval',
  'Source',
  'Trigger',
  'Operator',
  'Verifier'
] as const;
export type CardKind = (typeof CARD_KINDS)[number];

/** Non-color status pairing: the tone maps to a Badge glyph plus the label text. */
export type CardStatus = { label: string; tone: 'neutral' | 'ok' | 'warn' | 'danger' | 'running' };

/** One inventory row; `labels` backs the URL label filter, never a hierarchy. */
export type CardRow = {
  uid: string;
  name: string;
  kind: CardKind;
  version: string;
  space: string;
  status: CardStatus;
  owner: string;
  /** Human-relative label rendered in the table. */
  updated: string;
  /** ISO instant backing the sort order and `<time>` semantics. */
  updatedAt: string;
  labels: string[];
};

/** C-01 — the inventory projection: kind counts, matching rows, recent rail. */
export type CardsView = {
  total: number;
  kinds: { kind: CardKind; count: number }[];
  rows: CardRow[];
  recent: { uid: string; name: string; kind: CardKind; viewed: string }[];
};

/** One key/value line; `href` (tenant-relative) turns the value into a link. */
export type KV = { k: string; v: string; href?: string };

/** One typed Spec section for the shared fallback presentation. */
export type SpecSection = { title: string; note?: string; entries: KV[]; absent?: string };

/** One immutable version row; selecting a prior version re-renders read-only. */
export type CardVersionRow = { version: string; created: string; by: string; note: string };

/** One server-derived relationship, rendered as `relation — label`. */
export type CardRelationship = { relation: string; label: string; href?: string };

/** C-08 — the earned Workflow presentation payload: declaration, never results. */
export type WorkflowPresentation = {
  stages: string[];
  stageNote: string;
  io: KV[];
  governance: KV[];
  execution: string;
};

/** C-12 — the earned Verifier presentation payload; secrets never render. */
export type VerifierPresentation = {
  purpose: string;
  evidence: { kind: string; required: string; binding: string }[];
  evidenceNote: string;
  capabilities: KV[];
  secrets: string;
};

/**
 * One Overview chart slot for the Service workspace (C-09/S-01…S-07).
 * Every state is server-projected: `ok` draws the series, `gap` draws what
 * was observed and names the gap, `failed` and `nodata` render explicit
 * refusals — a chart never shows zero or health it does not have.
 */
export type ServiceChart = {
  title: string;
  measure: string;
  unit: string;
  /** Formatted latest value, e.g. `3.4 req/s` or `— · last 15:17`. */
  latest?: string;
  series: { label: string; points: number[] }[];
  /** Lower bound of the drawn y-domain for banded operational values. */
  min?: number;
  threshold?: { value: number; label: string };
  source: string;
  /** Canonical Observe link; `{range}` is stamped with the visible range. */
  link: { label: string; href: string };
  state: 'ok' | 'gap' | 'failed' | 'nodata';
  /** State-explaining line, e.g. `gap — not zero` or the failure sentence. */
  note?: string;
  /** Stable error code when the projection failed (S-06). */
  code?: string;
  /** ISO instant observations stop at, shortening the window (S-04). */
  endsAt?: string;
};

/**
 * One published Drift/Eval intelligence panel projected from the selected
 * version's declaration bindings (REQ-126). `state` renders unauthorized or
 * no-data refusals with the subject kept visible; `chart` is the score trend
 * against its threshold.
 */
export type ServiceSignal = {
  title: string;
  stamp?: string;
  verdict?: { label: string; tone: CardStatus['tone'] };
  subject?: string;
  chart?: {
    points: number[];
    min?: number;
    threshold: { value: number; label: string };
    /** Axis end labels, e.g. `['30d ago', 'today']`. */
    axis: [string, string];
    latest: string;
    qualifier: string;
  };
  context?: string;
  link?: { label: string; href: string };
  state?: 'unauthorized' | 'nodata';
  /** Prose lines for a refusal state, rendered instead of the trend. */
  body?: string[];
};

/** One server-projected Overview state variant (assessment through context). */
export type ServiceOverviewState = {
  assessment: {
    tone: CardStatus['tone'];
    title: string;
    sentence: string;
    meta: string;
    /** Renders the Retry affordance that clears only the failed projection. */
    retry?: boolean;
  };
  /** The four separated state channels: card, deployment, operational, freshness. */
  channels: { label: string; value: string }[];
  channelNote: string;
  charts: ServiceChart[];
  /** The restrained opt-in custom chart slot (title plus copy lines). */
  addChart: { title: string; lines: string[] };
  signals: ServiceSignal[];
  attention: {
    title: string;
    note: string;
    lines: string[];
    item?: { text: string; sub: string; href: string; label: string };
  };
  context: {
    components: { text: string; href?: string };
    activity: string;
    investigations: { label: string; href?: string; note?: string }[];
    foot: string;
  };
};

/** One Composition node; the drawer carries its contextual inspection. */
export type ServiceNode = {
  uid: string;
  kind: CardKind;
  name: string;
  version: string;
  status: string;
  note: string;
  drawer?: { headline: string; in?: string; out?: string; observe?: { label: string; href: string } };
};

/**
 * One drawn relationship in the Composition graph. `route` picks the wire
 * path: lane-to-lane elbow by default, `down` for a vertical edge inside one
 * lane, `under` for a long edge routed through the channel below the lanes.
 */
export type ServiceEdge = { from: string; to: string; label: string; route?: 'down' | 'under' };

/** S-02 — the deterministic linked-Card graph as ordered labeled lanes. */
export type ServiceComposition = {
  intro: string[];
  lanes: { title: string; note?: string; nodes: ServiceNode[] }[];
  edges: ServiceEdge[];
  aliasNote: string;
  foot: string;
};

/** S-03 — the declaration-first Definition presentation. */
export type ServiceDefinition = {
  declaration: { note: string; summary: string; entries: KV[] };
  components: {
    note: string;
    rows: {
      alias: string;
      ref: string;
      href: string;
      kind: string;
      publishes: string;
      /** Card route for a real publication target; absent for `—` rows. */
      publishesHref?: string;
    }[];
    aliasNote: string;
  };
  publications: {
    note: string;
    component: {
      label: string;
      /** Linked publication pairs: component Card → its Eval/Drift target. */
      flows: { from: { label: string; href: string }; to: { label: string; href: string } }[];
      /** Absence sentence rendered when a version declares no flows. */
      absent?: string;
    };
    service: { label: string; value: string; note: string };
  };
  rawSpec: { summary: string; meta: string; yaml: string; note: string };
  principal: { note: string; entries: KV[] };
  secrets: string;
};

/**
 * C-09/S-* — the earned Service operational workspace payload. `overview`
 * carries the projection's default state plus the mock-selectable variants;
 * `now` anchors every chart window. All states are server-authored — the
 * browser only selects which projected variant to render.
 */
export type ServicePresentation = {
  now: string;
  overview: { default: ServiceOverviewState; variants: Record<string, ServiceOverviewState> };
  composition: ServiceComposition;
  definition: ServiceDefinition;
};

/**
 * C-02 — the shared Card detail envelope every kind renders. `presentation`
 * carries a kind-specific payload only a registered workspace module reads;
 * every other kind uses the typed `spec` fallback sections.
 */
export type CardDetail = {
  uid: string;
  name: string;
  kind: CardKind;
  summary: string;
  status: CardStatus;
  version: string;
  currentVersion: string;
  versions: CardVersionRow[];
  actions: { label: string; href: string }[];
  spec: SpecSection[];
  presentation?: WorkflowPresentation | VerifierPresentation | ServicePresentation;
  metadata: KV[];
  relationships: CardRelationship[];
};
