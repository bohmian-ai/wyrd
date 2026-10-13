// Data model for the docs diagrams. A page names a diagram; the matching spec
// lives in this folder, keyed in `index.ts`. Captions and detail values are
// trusted, authored HTML (for <code> and <b>), rendered with {@html}.

export type Pt = [number, number];
export type Tone = 'progress' | 'ok' | 'warn' | 'danger' | 'muted';
export type EdgeKind = 'data' | 'control' | 'ref' | 'best';
export type NodeKind =
  | 'component'
  | 'client'
  | 'store'
  | 'declare'
  | 'evidence'
  | 'state'
  | 'decision'
  | 'table'
  | 'pod'
  | 'chip';

/** A label/value row in a figure's detail panel. Values are HTML. */
export type Row = [label: string, value: string];

export type DiagramNode = {
  id: string;
  kind: NodeKind;
  x: number;
  y: number;
  w: number;
  h: number;
  title: string;
  sub?: string;
  /** `state` nodes: border tone and the glyph that carries meaning without color. */
  tone?: Tone;
  glyph?: string;
  /** `table` nodes: columns, ◆ for a key and → for a foreign key. */
  cols?: { name: string; key?: boolean; fk?: boolean }[];
  /** Facet ids this node belongs to; a selected facet dims untagged-for-it peers. */
  tags?: string[];
  /** `chip` nodes: drawn dashed and muted because the part is absent. */
  off?: boolean;
  /** Click-for-detail rows. */
  detail?: Row[];
  /** `chip` nodes: the pod they sit in, whose dimming they share. */
  parent?: string;
};

export type DiagramEdge = {
  id: string;
  kind: EdgeKind;
  pts: Pt[];
  from?: string;
  to?: string;
  label?: string;
  lx?: number;
  ly?: number;
  anchor?: 'start' | 'middle' | 'end';
};

export type DiagramGroup = { x: number; y: number; w: number; h: number; label: string };
export type DiagramText = {
  x: number;
  y: number;
  text: string;
  anchor?: 'start' | 'middle' | 'end';
  /** One of the `wd-t-*` text styles; defaults to an edge label. */
  cls?: 'label' | 'group' | 'target' | 'sub' | 'yes' | 'no';
};
/** A dotted cobalt link drawn without arrowheads, such as the peer mTLS plane. */
export type DiagramLink = { d: string; labels: DiagramText[] };

export type DiagramView = {
  label?: string;
  size: [number, number];
  rest: string;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  groups?: DiagramGroup[];
  texts?: DiagramText[];
  links?: DiagramLink[];
};

export type DiagramStep = {
  label: string;
  nodes: string[];
  edges: string[];
  caption: string;
  /** Edge whose click selects this step. */
  edgeFor?: string;
  sql?: string;
  note?: string;
};

export type DiagramScenario = { name: string; path: string[]; say: string[] };
export type DiagramFacet = { id: string; label: string; caption: string; rows: Row[] };

export type DiagramSpec = {
  type: 'diagram';
  answers: string;
  lives: string;
  views: DiagramView[];
  steps?: DiagramStep[];
  scenarios?: DiagramScenario[];
  facets?: DiagramFacet[];
  detailHint?: string;
  narrLabel?: string;
};

export type SequenceParticipant = { id: string; kind: NodeKind; x: number; title: string; sub?: string };
export type SequenceMessage = {
  from: string;
  to: string;
  kind: EdgeKind;
  label: string;
  say: string;
  mark?: { at: string; text: string };
};
export type SequenceFlow = { name: string; rest: string; messages: SequenceMessage[] };
export type SequenceSpec = {
  type: 'sequence';
  answers: string;
  lives: string;
  width: number;
  participants: SequenceParticipant[];
  flows: SequenceFlow[];
};

/** One span of the timeline scrubber, from `from` (inclusive) to the next phase. */
export type TimelinePhase = {
  from: number;
  lane: number;
  /** Lane where a dashed placeholder shows the row's future home. */
  ghostLane?: number;
  readouts: { v: string; s: string }[];
  caption: string;
};
export type TimelineSpec = {
  type: 'timeline';
  answers: string;
  lives: string;
  lanes: string[];
  stages: { at: number; name: string; sub: string }[];
  readouts: string[];
  phases: TimelinePhase[];
  /** Units at the start of a phase spent moving between lanes. */
  move: number;
  start: number;
  /** Stop points used instead of continuous motion under reduced motion. */
  stops: number[];
};

export type FigureSpec = DiagramSpec | SequenceSpec | TimelineSpec;
