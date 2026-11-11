/**
 * Automatic layered layout of a Verifier's declared graph. Stages are columns:
 * inputs, steps by dependency rank, then whole-Verifier requirements. Boxes are
 * sized from declared text alone, so no later state can resize or move a node.
 */
import dagre from '@dagrejs/dagre';
import type { DefinitionGraph, DefinitionStep } from './verifier.js';

type Point = [number, number];

/** One positioned node. */
export interface LaidNode {
  readonly key: string;
  readonly type: 'input' | 'step' | 'req';
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A stage heading above the columns it names. */
export interface LaidStage {
  readonly text: string;
  readonly x: number;
  readonly width: number;
}

/** One dependency routed orthogonally from its source's boundary to its target's. */
export interface LaidEdge {
  readonly id: string;
  readonly source: string;
  readonly target: string;
  readonly points: readonly Point[];
  readonly label?: string;
  readonly labelAt?: { readonly x: number; readonly y: number; readonly anchor: 'start' | 'end' };
}

/** A laid-out definition and its drawing bounds. */
export interface LaidDefinition {
  readonly width: number;
  readonly height: number;
  readonly nodes: readonly LaidNode[];
  readonly stages: readonly LaidStage[];
  readonly edges: readonly LaidEdge[];
  /** The centre of every selectable element, attached requirement rows included, for keyboard movement. */
  readonly targets: readonly { readonly key: string; readonly x: number; readonly y: number }[];
}

// Fragment Mono advances 0.6em; Familjen Grotesk is budgeted at 0.56em.
const mono = (text: string, px: number) => text.length * px * 0.6;
const sans = (text: string, px: number) => text.length * px * 0.56;
const tag = (text: string, px: number) => mono(text, px) + 14;
const PAD = 26;
const MIN_W = 150;
const MAX_W = 340; // longer text ellipsizes inside its node
const STEP_H = 80;
const NODE_H = 52; // inputs and requirements: a title row and one line of detail
const REQ_ROW = 28;
const CHIP_ROW = 20;

const ROW_GAP = 16; // between nodes stacked in one column
const CHAIN_GAP = 40; // between the steps of a single chain, which stack vertically
const LANE = 14; // between vertical lanes in one gap
const ARROW = 8; // arrowhead length; every edge ends on its target's boundary
/** The stage heading band above the graph. */
export const HEAD = 34;
const CLEAR = 12; // clearance a corridor keeps from nodes and other horizontal runs

/** What a step says it waits for. */
export const needsText = (step: DefinitionStep) => (step.needs.length ? `needs ${step.needs.join(', ')}` : 'no step dependencies');

const stepWidth = (step: DefinitionStep) =>
  Math.max(
    mono(step.id, 13) + 6 + tag(step.kind, 10.4),
    mono(step.detail, 11),
    mono(needsText(step), 11),
    step.chips.reduce((sum, chip) => sum + tag(chip, 10) + 6, 0),
    ...step.requirements.map((each) => 18 + mono(each.id, 12) + 12)
  ) + PAD;

interface Box {
  id: string;
  type: LaidNode['type'];
  w: number;
  h: number;
  main: number;
  col: number;
  x: number;
  y: number;
  want: number;
}

interface Route {
  id: string;
  source: string;
  target: string;
  label?: string;
  sc: number;
  tc: number;
  long: boolean;
  bySource: boolean;
  firstLane: string;
  lastLane: string;
  sy: number;
  ty: number;
  cy: number;
}

/** One-dimensional overlap removal: keeps the given order and moves each cluster to the mean of its wishes. */
function pack(items: Box[], gap: number) {
  interface Cluster {
    items: Box[];
    wish: number[];
    size: number;
    top: number;
  }
  const clusters: Cluster[] = [];
  for (const item of items) {
    let cluster: Cluster = { items: [item], wish: [item.want - item.h / 2], size: item.h, top: item.want - item.h / 2 };
    for (let previous = clusters.at(-1); previous && previous.top + previous.size + gap > cluster.top; previous = clusters.at(-1)) {
      clusters.pop();
      const shifted = cluster.wish.map((wish) => wish - previous.size - gap);
      const wish = [...previous.wish, ...shifted];
      cluster = { items: [...previous.items, ...cluster.items], wish, size: previous.size + gap + cluster.size, top: mean(wish) };
    }
    clusters.push(cluster);
  }
  for (const cluster of clusters) {
    let y = cluster.top;
    for (const item of cluster.items) {
      item.y = y;
      y += item.h + gap;
    }
  }
}

const mean = (values: readonly number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;

/** All permutations of a short list, for ordering the few lanes in one gap. */
function* permutations<T>(list: T[]): Generator<T[]> {
  if (list.length <= 1) return void (yield list);
  for (let index = 0; index < list.length; index += 1)
    for (const rest of permutations([...list.slice(0, index), ...list.slice(index + 1)])) yield [list[index], ...rest];
}

/**
 * Lay a declared graph out once. Dagre ranks and spaces parallel steps; a
 * single chain of steps stacks vertically. Edges run orthogonally through
 * lanes in the gaps between columns, one port per lane, and an edge that
 * skips a column travels a corridor that clears every node.
 */
export function layoutDefinition(graph: DefinitionGraph): LaidDefinition {
  const boxes = new Map<string, Box>();
  const add = (id: string, type: Box['type'], w: number, h: number, main = h) =>
    boxes.set(id, { id, type, w: Math.ceil(Math.min(MAX_W, Math.max(w, MIN_W))), h, main, col: 0, x: 0, y: 0, want: 0 });
  const box = (id: string) => boxes.get(id) as Box;
  for (const input of graph.inputs) add(input.key, 'input', Math.max(mono(input.id, 13) + 6 + tag(input.kind, 11), sans(input.detail, 12)) + PAD, NODE_H);
  for (const step of graph.steps) {
    const main = STEP_H + (step.chips.length ? CHIP_ROW : 0);
    add(step.key, 'step', stepWidth(step), main + REQ_ROW * step.requirements.length, main);
  }
  for (const each of graph.requirements)
    add(each.key, 'req', Math.max(18 + mono(each.id, 13) + 6 + tag(each.kind, 10.4), sans(each.statement, 12)) + PAD, NODE_H);

  const steps = graph.steps.map((step) => step.key);
  const isStep = (id: string) => id.startsWith('step:');
  const stepEdges = graph.edges.filter((edge) => isStep(edge.source) && isStep(edge.target));
  const before = (id: string) => stepEdges.filter((edge) => edge.target === id).map((edge) => edge.source);
  const after = (id: string) => stepEdges.filter((edge) => edge.source === id).map((edge) => edge.target);

  /* Columns: inputs, step ranks, requirements. */
  const chain = steps.length >= 2 && steps.every((id) => before(id).length <= 1 && after(id).length <= 1) && steps.filter((id) => !before(id).length).length === 1;
  if (chain) {
    let y = 0;
    for (let id: string | undefined = steps.find((each) => !before(each).length); id; id = after(id)[0]) {
      const node = box(id);
      node.col = 1;
      node.y = y;
      y += node.h + CHAIN_GAP;
    }
  } else if (steps.length) {
    const ranked = new dagre.graphlib.Graph();
    ranked.setGraph({ rankdir: 'LR', ranksep: 60, nodesep: ROW_GAP + 8, marginx: 0, marginy: 0 });
    ranked.setDefaultEdgeLabel(() => ({}));
    for (const id of steps) ranked.setNode(id, { width: box(id).w, height: box(id).h });
    for (const edge of stepEdges) ranked.setEdge(edge.source, edge.target);
    dagre.layout(ranked);
    const ranks = [...new Set(steps.map((id) => Math.round(ranked.node(id).x)))].sort((a, b) => a - b);
    for (const id of steps) {
      const node = box(id);
      node.col = 1 + ranks.indexOf(Math.round(ranked.node(id).x));
      node.y = ranked.node(id).y - node.h / 2;
    }
  }
  const reqCol = steps.length ? Math.max(...steps.map((id) => box(id).col)) + 1 : 1;
  for (const each of graph.requirements) box(each.key).col = reqCol;
  const ncol = reqCol + 1;
  const vertical = (edge: { source: string; target: string }) => chain && isStep(edge.source) && isStep(edge.target);
  const routes: Route[] = graph.edges
    .filter((edge) => !vertical(edge))
    .map((edge) => ({ ...edge, id: `${edge.source}->${edge.target}`, sc: 0, tc: 0, long: false, bySource: false, firstLane: '', lastLane: '', sy: 0, ty: 0, cy: 0 }));
  const mid = (node: Box) => node.y + node.main / 2;

  /* Rows: inputs beside the steps they feed, requirements beside what they read; anything unread goes below. */
  let tail = Math.max(0, ...steps.map((id) => box(id).y + box(id).h));
  const beside = (nodes: Box[], neighbours: (node: Box) => Box[]) => {
    const wished = nodes.map((node, order) => {
      const mids = neighbours(node).map(mid);
      return { node, order, want: mids.length ? mean(mids) : Infinity };
    });
    for (const each of wished.filter((wish) => wish.want === Infinity)) {
      each.want = tail + NODE_H / 2;
      tail += NODE_H + ROW_GAP;
    }
    wished.sort((a, b) => a.want - b.want || a.order - b.order);
    pack(wished.map((each) => Object.assign(each.node, { want: each.want })), ROW_GAP);
  };
  beside(
    graph.inputs.map((input) => box(input.key)),
    (node) => routes.filter((route) => route.source === node.id && isStep(route.target)).map((route) => box(route.target))
  );
  tail = Math.max(0, ...steps.map((id) => box(id).y + box(id).h));
  beside(
    graph.requirements.map((each) => box(each.key)),
    (node) => routes.filter((route) => route.target === node.id).map((route) => box(route.source))
  );

  /* Groups: a fan-out shares its source's lane, a fan-in its target's lane. */
  const outdegree = (id: string) => routes.filter((route) => route.source === id).length;
  for (const route of routes) {
    const sc = box(route.source).col;
    const tc = box(route.target).col;
    const bySource = outdegree(route.source) > 1;
    const group = bySource ? `s:${route.source}` : `t:${route.target}`;
    const srcLane = bySource ? group : `${group}|${route.source}`;
    const tgtLane = bySource ? `${group}|${tc}` : group;
    const long = tc > sc + 1;
    const lastLane = long || !bySource ? tgtLane : srcLane;
    Object.assign(route, { sc, tc, long, bySource, lastLane, firstLane: long ? srcLane : lastLane });
  }

  /* Ports: one per lane on each side, ordered by where the other end lies. */
  const ports = new Map<string, number>();
  const spread = (node: Box, side: 'in' | 'out', lane: (route: Route) => string, far: (route: Route) => Box, own: Route[]) => {
    const keys = [...new Set(own.map(lane))]
      .map((key) => [key, mean(own.filter((route) => lane(route) === key).map((route) => mid(far(route))))] as const)
      .sort((a, b) => a[1] - b[1]);
    keys.forEach(([key], index) => ports.set(`${side}:${node.id}:${key}`, node.y + (node.main * (index + 1)) / (keys.length + 1)));
  };
  for (const node of boxes.values()) {
    spread(node, 'out', (route) => route.firstLane, (route) => box(route.target), routes.filter((route) => route.source === node.id));
    spread(node, 'in', (route) => route.lastLane, (route) => box(route.source), routes.filter((route) => route.target === node.id));
  }
  for (const route of routes) {
    route.sy = ports.get(`out:${route.source}:${route.firstLane}`) as number;
    route.ty = ports.get(`in:${route.target}:${route.lastLane}`) as number;
  }

  /* Gap widths from lane counts and labels, then column x positions. */
  const all = [...boxes.values()];
  const labelWidth = (label?: string) => (label ? mono(label, 10) + 8 : 0);
  const colW = Array.from({ length: ncol }, (_, col) => Math.max(0, ...all.filter((node) => node.col === col).map((node) => node.w)));
  const lanesIn = Array.from({ length: ncol - 1 }, () => new Set<string>());
  const labelL = Array<number>(ncol - 1).fill(0);
  const labelR = Array<number>(ncol - 1).fill(0);
  for (const route of routes) {
    lanesIn[route.sc].add(route.firstLane);
    lanesIn[route.tc - 1].add(route.lastLane);
    if (route.bySource) labelL[route.sc] = Math.max(labelL[route.sc], labelWidth(route.label));
    else labelR[route.tc - 1] = Math.max(labelR[route.tc - 1], labelWidth(route.label));
  }
  const leftPad = (gap: number) => 14 + labelL[gap];
  const gapW = lanesIn.map((lanes, gap) => Math.max(44, leftPad(gap) + Math.max(0, lanes.size - 1) * LANE + Math.max(22, 14 + labelR[gap]) + ARROW));
  const colX = [0];
  for (let col = 1; col < ncol; col += 1) colX[col] = colX[col - 1] + colW[col - 1] + gapW[col - 1];
  for (const node of all) {
    node.x = colX[node.col];
    node.w = colW[node.col];
  }
  const right = (col: number) => colX[col] + colW[col];

  /* Corridors for edges that skip a column: the nearest y that clears nodes and other horizontal runs. */
  const runs = routes
    .filter((route) => !route.long)
    .flatMap((route) => [
      { y: route.sy, gap: route.sc, lane: route.firstLane },
      { y: route.ty, gap: route.sc, lane: route.lastLane }
    ]);
  const corridors = new Map<string, number>();
  for (const route of routes.filter((each) => each.long)) {
    const key = `${route.firstLane}>${route.lastLane}`;
    if (!corridors.has(key)) {
      const between = all.filter((node) => node.col > route.sc && node.col < route.tc);
      // A run of the same lane is this edge's own shared stub, not an obstacle.
      const blocked = (y: number) =>
        between.some((node) => y > node.y - CLEAR && y < node.y + node.h + CLEAR) ||
        runs.some((run) => run.lane !== route.firstLane && run.lane !== route.lastLane && run.gap >= route.sc && run.gap <= route.tc - 1 && Math.abs(run.y - y) < CLEAR);
      const candidates = [route.sy, route.ty, ...between.flatMap((node) => [node.y - CLEAR - 2, node.y + node.h + CLEAR + 2])]
        .flatMap((y) => [y, y - 2 * CLEAR, y + 2 * CLEAR])
        .sort((a, b) => Math.abs(a - route.sy) - Math.abs(b - route.sy));
      const y = candidates.find((candidate) => !blocked(candidate)) ?? Math.max(...between.map((node) => node.y + node.h)) + 3 * CLEAR;
      corridors.set(key, y);
      for (let gap = route.sc; gap < route.tc; gap += 1) runs.push({ y, gap, lane: key });
    }
    route.cy = corridors.get(key) as number;
  }

  /* Lane order per gap: the permutation with the fewest crossings and no collinear overlap. */
  const laneX = new Map<string, number>();
  lanesIn.forEach((keys, gap) => {
    const lanes = [...keys].map((key) => {
      const left: number[] = [];
      const rightYs: number[] = [];
      for (const route of routes) {
        if (route.sc === gap && route.firstLane === key) {
          left.push(route.sy);
          rightYs.push(route.long ? route.cy : route.ty);
        }
        if (route.long && route.tc - 1 === gap && route.lastLane === key) {
          left.push(route.cy);
          rightYs.push(route.ty);
        }
      }
      return { key, left, right: rightYs, top: Math.min(...left, ...rightYs), bottom: Math.max(...left, ...rightYs) };
    });
    let best = lanes;
    let bestCost = Infinity;
    // Exhaustive ordering up to six lanes per gap; wider gaps keep declaration order.
    for (const order of lanes.length <= 6 ? permutations(lanes) : [lanes]) {
      let cost = 0;
      order.forEach((lane, i) =>
        order.forEach((other, j) => {
          if (i === j) return;
          for (const y of lane.left) {
            if (j < i && y > other.top + 1 && y < other.bottom - 1) cost += 1;
            if (j < i && other.right.some((each) => Math.abs(each - y) < 4)) cost += 10;
          }
          for (const y of lane.right) if (j > i && y > other.top + 1 && y < other.bottom - 1) cost += 1;
        })
      );
      if (cost < bestCost) [best, bestCost] = [order, cost];
    }
    best.forEach((lane, index) => laneX.set(`${gap}:${lane.key}`, right(gap) + leftPad(gap) + index * LANE));
  });

  /* Paths: orthogonal points from the source boundary to the target boundary. */
  const routed = new Map(routes.map((route) => [route.id, route]));
  const seen = new Set<string>();
  const edges = graph.edges.map((edge): LaidEdge => {
    const id = `${edge.source}->${edge.target}`;
    const source = box(edge.source);
    const target = box(edge.target);
    const route = routed.get(id);
    let points: Point[];
    let labelAt: LaidEdge['labelAt'];
    if (!route) {
      const x = source.x + source.w / 2;
      points = [[x, source.y + source.h], [x, target.y]];
      labelAt = { x: x + 8, y: (source.y + source.h + target.y) / 2, anchor: 'start' };
    } else {
      const sx = source.x + source.w;
      const a = laneX.get(`${route.sc}:${route.firstLane}`) as number;
      const b = laneX.get(`${route.tc - 1}:${route.lastLane}`) as number;
      points = route.long
        ? [[sx, route.sy], [a, route.sy], [a, route.cy], [b, route.cy], [b, route.ty], [target.x, route.ty]]
        : [[sx, route.sy], [a, route.sy], [a, route.ty], [target.x, route.ty]];
      labelAt = route.bySource ? { x: sx + 6, y: route.sy - 7, anchor: 'start' } : { x: target.x - ARROW - 4, y: route.ty - 7, anchor: 'end' };
    }
    // Zero-length segments go, so corners are drawn only where the route turns.
    points = points.filter((point, index) => index === 0 || point[0] !== points[index - 1][0] || point[1] !== points[index - 1][1]);
    // Each lane shows a label once: a fan-out on the shared stub, a fan-in on the shared final run.
    const spot = `${edge.label}@${Math.round(labelAt.x)},${Math.round(labelAt.y)}`;
    const labelled = edge.label !== undefined && !seen.has(spot);
    seen.add(spot);
    return { id, source: edge.source, target: edge.target, points, ...(labelled ? { label: edge.label, labelAt } : {}) };
  });

  /* Normalise to the origin and leave the heading band above. */
  const ys = [...all.flatMap((node) => [node.y, node.y + node.h]), ...corridors.values()];
  const top = ys.length ? Math.min(...ys) : 0;
  const bottom = ys.length ? Math.max(...ys) : 0;
  const dy = HEAD - top;
  const stage = (text: string, from: number, to: number): LaidStage => ({ text, x: colX[from], width: right(to) - colX[from] });
  return {
    width: right(ncol - 1),
    height: bottom - top + HEAD,
    nodes: all.map((node) => ({ key: node.id, type: node.type, x: node.x, y: node.y + dy, width: node.w, height: node.h })),
    stages: [
      stage('Inputs', 0, 0),
      ...(steps.length ? [stage('Steps', 1, reqCol - 1)] : []),
      stage(steps.length ? 'After all steps' : 'Requirements', reqCol, reqCol)
    ],
    targets: [
      ...all.map((node) => ({ key: node.id, x: node.x + node.w / 2, y: node.y + dy + node.main / 2 })),
      ...graph.steps.flatMap((step) =>
        step.requirements.map((each, row) => ({ key: each.key, x: box(step.key).x + box(step.key).w / 2, y: box(step.key).y + dy + box(step.key).main + REQ_ROW * (row + 0.5) }))
      )
    ],
    edges: edges.map((edge) => ({
      ...edge,
      points: edge.points.map(([x, y]): Point => [x, y + dy]),
      ...(edge.labelAt ? { labelAt: { ...edge.labelAt, y: edge.labelAt.y + dy } } : {})
    }))
  };
}
