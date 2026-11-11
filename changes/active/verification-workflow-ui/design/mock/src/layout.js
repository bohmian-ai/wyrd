import dagre from '@dagrejs/dagre';

// Node boxes are sized from definition text alone (Fragment Mono advances 0.6em; Familjen Grotesk is budgeted at
// 0.56em), so a status or output change can never resize or move a node.
const mono = (s, px) => s.length * px * 0.6;
const sans = (s, px) => s.length * px * 0.56;
const tag = (s, px) => mono(s, px) + 14;
const PAD = 26;
const STEP_H = 80;
const NODE_H = 52; // inputs, member runs and requirements: a title row and one line of detail
const REQ_ROW = 28;
const CHIP_ROW = 20;
const STATUS = 'waiting for approval'; // the longest step status word
const OUTCOME = 'not judged yet'; // the longest requirement outcome word

const ROW_GAP = 16; // between nodes stacked in one column
const CHAIN_GAP = 40; // between steps of a single chain, which stack vertically like the original policy canvas
const LANE = 14; // between vertical lanes in one gap
const ARROW = 8; // arrowhead length; every edge ends on its target's boundary
const HEAD = 34; // stage heading band above the graph
const CLEAR = 12; // clearance a corridor keeps from nodes and other horizontal segments

// Fan-out, repeat and admission chips sit on their own row, so a step's width never sums them with its status.
const chipsOf = (s) => [s.fanout && '× 99 items', s.repeat && '↻ 9 iterations', s.when && 'discarded'].filter(Boolean);
function stepWidth(id, s) {
  return Math.max(
    mono(id, 13) + 6 + tag(s.kind, 10.4),
    mono(s.ref, 11),
    mono(STATUS, 11) + 17,
    chipsOf(s).reduce((n, c) => n + tag(c, 10) + 6, 0),
    ...s.reqs.map((r) => 18 + mono(r, 12) + 12 + mono(OUTCOME, 11)),
  ) + PAD;
}
const inputWidth = (id, i) => Math.max(mono(id, 13) + 6 + tag(i.kind, 11), sans(i.sub, 12)) + PAD;
const reqWidth = (id, q) => Math.max(18 + mono(id, 13) + 6 + tag(q.kind, 10.4), sans(q.desc, 12), mono(OUTCOME, 11)) + PAD;
const labelWidth = (s) => (s ? mono(s, 10) + 8 : 0);

/** One-dimensional overlap removal: keeps the given order and moves each cluster to the mean of its wishes. */
function pack(items, gap) {
  const clusters = [];
  for (const it of items) {
    let c = { items: [it], wish: [it.want - it.h / 2], size: it.h };
    c.top = c.wish[0];
    while (clusters.length) {
      const p = clusters.at(-1);
      if (p.top + p.size + gap <= c.top) break;
      clusters.pop();
      const shifted = c.wish.map((w) => w - p.size - gap);
      c = { items: [...p.items, ...c.items], wish: [...p.wish, ...shifted], size: p.size + gap + c.size };
      c.top = c.wish.reduce((a, b) => a + b, 0) / c.wish.length;
    }
    clusters.push(c);
  }
  for (const c of clusters) {
    let y = c.top;
    for (const it of c.items) { it.y = y; y += it.h + gap; }
  }
}

/** All permutations of a short list, for ordering the few lanes in one gap. */
function* perms(a) {
  if (a.length <= 1) { yield a; return; }
  for (let i = 0; i < a.length; i++) for (const p of perms([...a.slice(0, i), ...a.slice(i + 1)])) yield [a[i], ...p];
}

/**
 * Lays a definition out once and returns Svelte Flow nodes and edges plus the drawing bounds.
 * Stages are columns (inputs or member runs, steps, requirements judged after all steps). Dagre orders and spaces
 * parallel steps; a single chain of steps stacks vertically. Edges are routed orthogonally through lanes in the gaps
 * between columns, with one port per lane, and every edge ends with an arrowhead on its target's boundary. Edges that
 * skip a column travel along a corridor that clears every node. A session's pinned members share one bus.
 * `members` are the pinned member executions of a session judgment. Progress never enters this function.
 */
export function layoutDefinition(def, members = []) {
  const N = new Map();
  const add = (id, type, w, h, main = h) => N.set(id, { id, type, w: Math.ceil(Math.max(w, 150)), h, main, col: 0, y: 0 });
  const busOf = {};
  const chrono = [...members].sort((a, b) => a.at - b.at);
  for (const [id, i] of Object.entries(def.inputs)) {
    if (i.members && chrono.length) for (const m of chrono) { add(m.id, 'member', 210, NODE_H); busOf[m.id] = id; }
    else add(id, 'input', inputWidth(id, i), NODE_H);
  }
  const steps = Object.keys(def.steps);
  for (const id of steps) {
    const s = def.steps[id], main = STEP_H + (chipsOf(s).length ? CHIP_ROW : 0);
    add(id, 'step', stepWidth(id, s), main + REQ_ROW * s.reqs.length, main);
  }
  for (const id of def.after) add(id, 'req', reqWidth(id, def.reqs[id]), NODE_H);

  const expand = (id) => (def.inputs[id]?.members && chrono.length ? chrono.map((m) => m.id) : [id]);
  const E = def.deps.flatMap(([s, t, label]) => expand(s).map((source) => ({ id: `${source}->${t}`, source, target: t, label, bus: busOf[source] })));

  /* Columns: inputs, step ranks, requirements. */
  const stepDeps = (s) => def.steps[s].deps.filter((d) => d in def.steps);
  const dependents = (s) => steps.filter((t) => stepDeps(t).includes(s));
  const chain = steps.length >= 2 && steps.every((s) => stepDeps(s).length <= 1 && dependents(s).length <= 1)
    && steps.filter((s) => !stepDeps(s).length).length === 1;
  if (chain) {
    let s = steps.find((x) => !stepDeps(x).length), y = 0;
    while (s) { const n = N.get(s); n.col = 1; n.y = y; y += n.h + CHAIN_GAP; s = dependents(s)[0]; }
  } else if (steps.length) {
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'LR', ranksep: 60, nodesep: ROW_GAP + 8, marginx: 0, marginy: 0 });
    g.setDefaultEdgeLabel(() => ({}));
    for (const s of steps) g.setNode(s, { width: N.get(s).w, height: N.get(s).h });
    for (const s of steps) for (const d of stepDeps(s)) g.setEdge(d, s);
    dagre.layout(g);
    const ranks = [...new Set(steps.map((s) => Math.round(g.node(s).x)))].sort((a, b) => a - b);
    for (const s of steps) { const n = N.get(s); n.col = 1 + ranks.indexOf(Math.round(g.node(s).x)); n.y = g.node(s).y - n.h / 2; }
  }
  const reqCol = steps.length ? Math.max(...steps.map((s) => N.get(s).col)) + 1 : 1;
  for (const id of def.after) N.get(id).col = reqCol;
  const ncol = reqCol + 1;
  const vertical = (e) => chain && e.source in def.steps && e.target in def.steps;
  const H = E.filter((e) => !vertical(e));
  const mid = (n) => n.y + n.main / 2;

  /* Rows: inputs beside the steps they feed, requirements beside what they read. */
  const inputs = [...N.values()].filter((n) => n.col === 0);
  if (chrono.length) pack(inputs.map((n, k) => Object.assign(n, { want: k * (NODE_H + ROW_GAP) + NODE_H / 2 })), ROW_GAP);
  else {
    const fed = inputs.map((n, k) => {
      const into = H.filter((e) => e.source === n.id && e.target in def.steps).map((e) => mid(N.get(e.target)));
      return { n, k, want: into.length ? into.reduce((a, b) => a + b, 0) / into.length : Infinity };
    });
    let tail = Math.max(0, ...[...N.values()].filter((n) => n.type === 'step').map((n) => n.y + n.h));
    for (const f of fed.filter((f) => f.want === Infinity)) { f.want = tail + NODE_H / 2; tail += NODE_H + ROW_GAP; }
    fed.sort((a, b) => a.want - b.want || a.k - b.k);
    pack(fed.map((f) => Object.assign(f.n, { want: f.want })), ROW_GAP);
  }
  const reqs = def.after.map((id, k) => {
    const from = H.filter((e) => e.target === id).map((e) => mid(N.get(e.source)));
    return { n: N.get(id), k, want: from.reduce((a, b) => a + b, 0) / from.length };
  }).sort((a, b) => a.want - b.want || a.k - b.k);
  pack(reqs.map((r) => Object.assign(r.n, { want: r.want })), ROW_GAP);

  /* Groups: a fan-out shares its source's lane, a fan-in its target's lane, a session's members one bus. */
  const outdeg = (id) => H.filter((e) => e.source === id).length;
  for (const e of H) {
    const sc = N.get(e.source).col, tc = N.get(e.target).col;
    const bySource = !e.bus && outdeg(e.source) > 1;
    const group = e.bus ? `b:${e.bus}` : bySource ? `s:${e.source}` : `t:${e.target}`;
    Object.assign(e, { sc, tc, long: tc > sc + 1, bySource,
      srcLane: bySource || e.bus ? group : `${group}|${e.source}`,
      tgtLane: bySource ? `${group}|${tc}` : group });
    e.lastLane = e.long ? e.tgtLane : (bySource ? e.srcLane : e.tgtLane);
    e.firstLane = e.long ? e.srcLane : e.lastLane;
  }

  /* Ports: one per lane on each side, ordered by where the other end lies. */
  const portY = {};
  const spread = (n, keys, side) => keys.forEach((k, i) => { portY[`${side}:${n.id}:${k}`] = n.y + (n.main * (i + 1)) / (keys.length + 1); });
  for (const n of N.values()) {
    const outs = H.filter((e) => e.source === n.id);
    const outKeys = [...new Set(outs.map((e) => e.firstLane))]
      .map((k) => [k, outs.filter((e) => e.firstLane === k).reduce((a, e) => a + mid(N.get(e.target)), 0) / outs.filter((e) => e.firstLane === k).length])
      .sort((a, b) => a[1] - b[1]).map(([k]) => k);
    spread(n, outKeys, 'out');
    const ins = H.filter((e) => e.target === n.id);
    const inKeys = [...new Set(ins.map((e) => e.lastLane))]
      .map((k) => [k, ins.filter((e) => e.lastLane === k).reduce((a, e) => a + mid(N.get(e.source)), 0) / ins.filter((e) => e.lastLane === k).length])
      .sort((a, b) => a[1] - b[1]).map(([k]) => k);
    spread(n, inKeys, 'in');
  }
  for (const e of H) { e.sy = portY[`out:${e.source}:${e.firstLane}`]; e.ty = portY[`in:${e.target}:${e.lastLane}`]; }

  /* Gap widths from lane counts and labels, then column x positions. */
  const colW = Array.from({ length: ncol }, (_, c) => Math.max(0, ...[...N.values()].filter((n) => n.col === c).map((n) => n.w)));
  const lanesIn = Array.from({ length: ncol - 1 }, () => new Set());
  const labelL = Array(ncol - 1).fill(0), labelR = Array(ncol - 1).fill(0);
  for (const e of H) {
    lanesIn[e.sc].add(e.firstLane);
    lanesIn[e.tc - 1].add(e.lastLane);
    if (e.label && e.bySource) labelL[e.sc] = Math.max(labelL[e.sc], labelWidth(e.label));
    if (e.label && !e.bySource) labelR[e.tc - 1] = Math.max(labelR[e.tc - 1], labelWidth(e.label));
  }
  const leftPad = (g) => 14 + labelL[g];
  const gapW = lanesIn.map((L, g) => Math.max(44,leftPad(g) + Math.max(0, L.size - 1) * LANE + Math.max(22, 14 + labelR[g]) + ARROW));
  const colX = [0];
  for (let c = 1; c < ncol; c++) colX[c] = colX[c - 1] + colW[c - 1] + gapW[c - 1];
  for (const n of N.values()) { n.x = colX[n.col]; n.w = colW[n.col]; }
  const right = (c) => colX[c] + colW[c];

  /* Corridors for edges that skip a column: the nearest y that clears nodes and other horizontal runs. */
  const runs = H.filter((e) => !e.long).flatMap((e) => [{ y: e.sy, g: e.sc, k: e.firstLane }, { y: e.ty, g: e.sc, k: e.lastLane }]);
  const corridor = {};
  for (const e of H.filter((x) => x.long)) {
    const key = `${e.firstLane}>${e.lastLane}`;
    if (corridor[key] != null) continue;
    const between = [...N.values()].filter((n) => n.col > e.sc && n.col < e.tc);
    // A run of the same lane is this edge's own shared stub, not an obstacle.
    const blocked = (y) => between.some((n) => y > n.y - CLEAR && y < n.y + n.h + CLEAR)
      || runs.some((r) => r.k !== e.firstLane && r.k !== e.lastLane && r.g >= e.sc && r.g <= e.tc - 1 && Math.abs(r.y - y) < CLEAR);
    const cands = [e.sy, e.ty, ...between.flatMap((n) => [n.y - CLEAR - 2, n.y + n.h + CLEAR + 2])]
      .flatMap((y) => [y, y - 2 * CLEAR, y + 2 * CLEAR]).sort((a, b) => Math.abs(a - e.sy) - Math.abs(b - e.sy));
    const y = cands.find((c) => !blocked(c)) ?? Math.max(...between.map((n) => n.y + n.h)) + 3 * CLEAR;
    corridor[key] = y;
    for (let g = e.sc; g < e.tc; g++) runs.push({ y, g, k: key });
  }
  for (const e of H) if (e.long) e.cy = corridor[`${e.firstLane}>${e.lastLane}`];

  /* Lane order per gap: the permutation with the fewest crossings and no collinear overlap. */
  const laneX = {};
  lanesIn.forEach((L, g) => {
    const lanes = [...L].map((k) => {
      const left = [], rightYs = [];
      for (const e of H) {
        if (e.sc === g && e.firstLane === k) { left.push(e.sy); rightYs.push(e.long ? e.cy : e.ty); }
        if (e.long && e.tc - 1 === g && e.lastLane === k) { left.push(e.cy); rightYs.push(e.ty); }
      }
      const all = [...left, ...rightYs];
      return { k, left, right: rightYs, a: Math.min(...all), b: Math.max(...all) };
    });
    let best = lanes, bestCost = Infinity;
    for (const p of lanes.length <= 6 ? perms(lanes) : [lanes]) {
      let cost = 0;
      p.forEach((li, i) => p.forEach((lj, j) => {
        if (i === j) return;
        for (const y of li.left) {
          if (j < i && y > lj.a + 1 && y < lj.b - 1) cost += 1;
          if (j < i && lj.right.some((r) => Math.abs(r - y) < 4)) cost += 10;
        }
        for (const y of li.right) if (j > i && y > lj.a + 1 && y < lj.b - 1) cost += 1;
      }));
      if (cost < bestCost) { best = p; bestCost = cost; }
    }
    best.forEach((l, i) => { laneX[`${g}:${l.k}`] = right(g) + leftPad(g) + i * LANE; });
  });

  /* Paths: orthogonal points from the source boundary to the target boundary. */
  const edges = E.map((e) => {
    const s = N.get(e.source), t = N.get(e.target);
    let points, labelAt = null;
    if (vertical(e)) {
      const x = s.x + s.w / 2;
      points = [[x, s.y + s.h], [x, t.y]];
      if (e.label) labelAt = { x: x + 8, y: (s.y + s.h + t.y) / 2, anchor: 'start' };
    } else {
      const sx = s.x + s.w, tx = t.x;
      const a = laneX[`${e.sc}:${e.firstLane}`];
      if (!e.long) points = [[sx, e.sy], [a, e.sy], [a, e.ty], [tx, e.ty]];
      else {
        const b = laneX[`${e.tc - 1}:${e.lastLane}`];
        points = [[sx, e.sy], [a, e.sy], [a, e.cy], [b, e.cy], [b, e.ty], [tx, e.ty]];
      }
      if (e.label) labelAt = e.bySource ? { x: sx + 6, y: e.sy - 7, anchor: 'start' } : { x: tx - ARROW - 4, y: e.ty - 7, anchor: 'end' };
    }
    // Drop zero-length segments so corners are only drawn where the route turns.
    points = points.filter((p, i) => i === 0 || p[0] !== points[i - 1][0] || p[1] !== points[i - 1][1]);
    return {
      id: e.id, source: e.source, target: e.target, type: 'dep', deletable: false, selectable: false, focusable: false,
      data: { points, label: e.label, labelAt, bus: e.bus ?? null },
    };
  });
  // Show each lane's label once: fan-out labels on the shared stub, fan-in labels on the shared final run.
  const seen = new Set();
  for (const e of edges) {
    const key = e.data.labelAt && `${e.data.label}@${Math.round(e.data.labelAt.x)},${Math.round(e.data.labelAt.y)}`;
    if (key && seen.has(key)) e.data.labelAt = null;
    else if (key) seen.add(key);
  }

  /* Normalise to the origin and add stage headings above each column group. */
  const ys = [...N.values()].flatMap((n) => [n.y, n.y + n.h]).concat(Object.values(corridor));
  const top = Math.min(...ys), bottom = Math.max(...ys);
  const dy = HEAD - top;
  for (const n of N.values()) n.y += dy;
  for (const e of edges) {
    e.data.points = e.data.points.map(([x, y]) => [x, y + dy]);
    if (e.data.labelAt) e.data.labelAt.y += dy;
  }
  const stage = (id, text, c0, c1) => ({ id: `stage:${id}`, type: 'stage', data: { text }, position: { x: colX[c0], y: 0 }, width: right(c1) - colX[c0], height: 18,
    draggable: false, connectable: false, selectable: false, deletable: false, focusable: false });
  const stages = [stage('in', chrono.length ? 'Member runs' : 'Inputs', 0, 0)];
  if (steps.length) stages.push(stage('steps', 'Steps', 1, reqCol - 1));
  stages.push(stage('after', steps.length ? 'After all steps' : 'Requirements', reqCol, reqCol));

  return {
    width: right(ncol - 1),
    height: bottom - top + HEAD,
    nodes: [...stages, ...[...N.values()].map((n) => ({
      id: n.id, type: n.type, width: n.w, height: n.h, data: {}, position: { x: n.x, y: n.y },
      draggable: false, connectable: false, selectable: false, deletable: false, focusable: false,
    }))],
    edges,
  };
}
