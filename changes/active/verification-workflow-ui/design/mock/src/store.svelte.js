import { tick } from 'svelte';
import { DEFS, EVENTS, REPOS, REVIEW_SETS, buildExecutions, finishRunning } from './catalog.js';
import { NOW, instKey, O, clock } from './data.js';

/** Sample executions keyed by exact id: the mock's stand-in for server state. Graph nodes read state from here. */
export const execs = $state(buildExecutions());
/** Recorded review sets. Membership is fixture data, never inferred from time or head. */
export const sets = $state(REVIEW_SETS.map((s) => ({ ...s })));

/** Page state. The URL hash mirrors everything a link must reproduce. */
export const ui = $state({
  route: 'verify', // verify | exec | repos | verifiers | repo | undeclared | pull | verifier | settings | missing
  f: [], range: '24h', brush: null, drag: null, group: 'none', live: true, held: [], shown: 50, scroll: 0, last: '',
  x: '', sel: '', inst: '', tab: '', out: '', trace: '', cite: '', back: '',
  vdef: '', vtab: 'graph',
  card: '', v: '', dtab: 'yaml', reviews: 'latest', q: '', show: 'all', ghid: '', pr: 0, set: '', missing: '',
  fallback: '', // text to copy by hand when the clipboard is unavailable
  here: '#/verify', // the current page's hash, reactive, so links that return here stay exact
  role: 'admin', conn: 'connected', respond: 'accepted', race: 'cancelled', events: 0, busy: '', notice: {},
});

export const FIELDS = ['status', 'verdict', 'verifier', 'failed', 'card', 'service', 'run', 'session', 'repo', 'pr'];
export const RANGES = { '1h': ['Last hour', 3600000], '6h': ['Last 6 hours', 6 * 3600000], '24h': ['Last 24 hours', 86400000], '7d': ['Last 7 days', 7 * 86400000], '14d': ['Last 14 days', 14 * 86400000] };

/** The value a row has for a filter field. Absent values are the explicit `none`. */
export function valuesOf(x, field) {
  switch (field) {
    case 'verdict': return [x.verdict ?? 'none'];
    case 'verifier': return [x.def];
    case 'failed': return failedOf(x);
    case 'pr': return [x.pr ? `${x.repo}#${x.pr}` : 'none'];
    default: return [x[field] ?? 'none'];
  }
}
export const failedOf = (x) => Object.entries(x.reqs).filter(([, r]) => r.outcome === 'failed').map(([id]) => id);
/** Unfinished work a step is blocked on, shown beside the status (never as a verdict). */
export const blockedOf = (x) => [...new Set(Object.values(x.steps).map((s) => s.action).filter((a) => a.startsWith('awaiting')))];

export function timeWindow() {
  return ui.brush ?? [NOW - RANGES[ui.range][1], NOW];
}
/** The one filtered population that the counts, facets, both charts and the table all describe. */
export function population() {
  const [from, to] = timeWindow();
  return Object.values(execs)
    .filter((x) => !x.hidden && !ui.held.includes(x.id) && x.at >= from && x.at <= to)
    .filter((x) => FIELDS.every((k) => { const want = ui.f.filter(([f]) => f === k).map(([, v]) => v); return !want.length || valuesOf(x, k).some((v) => want.includes(v)); }))
    .sort((a, b) => b.at - a.at || a.id.localeCompare(b.id));
}

export function current() {
  if (ui.route === 'verifier') return { def: DEFS[ui.vdef], exec: null };
  const exec = ui.route === 'exec' ? execs[ui.x] : null;
  return exec ? { def: DEFS[exec.def], exec } : { def: null, exec: null };
}
export const selection = () => {
  const i = ui.sel.indexOf(':');
  return { type: ui.sel.slice(0, i), id: ui.sel.slice(i + 1) };
};
export const reqState = (id) => current().exec?.reqs[id] ?? { outcome: 'none' };

/** The current invocation of a step: the chosen one if it still exists, else the latest. */
export function currentInst(stepId) {
  const inst = current().exec.steps[stepId].inst;
  return inst.find((i) => instKey(i) === ui.inst) ?? inst.at(-1);
}

function valid(sel, def, exec) {
  if (sel === '' || (sel === 'execution:' && exec)) return true;
  const [type, id] = [sel.slice(0, sel.indexOf(':')), sel.slice(sel.indexOf(':') + 1)];
  return (type === 'step' && id in def.steps) || (type === 'req' && id in def.reqs) || (type === 'input' && id in def.inputs)
    || (type === 'member' && !!exec?.members?.includes(id));
}

/** Where an execution opens: its first failed requirement, else a decision it waits for, else running work, else its outputs. */
function focusOf(x) {
  const d = DEFS[x.def];
  const failed = [...Object.values(d.steps).flatMap((s) => s.reqs), ...d.after].find((r) => x.reqs[r]?.outcome === 'failed');
  if (failed) return `req:${failed}`;
  const step = Object.keys(x.steps).find((s) => x.steps[s].action === 'awaiting_approval') ?? Object.keys(x.steps).find((s) => x.steps[s].action === 'running');
  if (step) return `step:${step}`;
  return Object.keys(d.steps).length ? 'execution:' : `req:${d.after[0]}`;
}

export function select(sel, { inst, tab, out } = {}) {
  const { exec } = current();
  ui.sel = sel;
  ui.trace = '';
  const { type, id } = selection();
  if (type === 'step' && exec) {
    const list = exec.steps[id].inst;
    const chosen = list.find((i) => instKey(i) === inst) ?? list.at(-1);
    ui.inst = chosen ? instKey(chosen) : '';
    ui.out = out ?? chosen?.outs[0]?.name ?? '';
    ui.tab = tab ?? (out ? 'outputs' : 'overview');
  } else {
    ui.inst = '';
    ui.out = '';
    ui.tab = tab ?? (type === 'execution' ? 'outputs' : 'overview');
  }
  sync();
}

/** Selects the exact producer of a publication: step, invocation, and output name. */
export function openOutput(step, inst, out) { select(`step:${step}`, { inst, tab: 'outputs', out }); }
export function openTrace(span) { ui.trace = span; sync(); }

/* ---------- Links ---------- */

function selParams(p) {
  if (ui.sel) p.set('sel', ui.sel);
  if (ui.tab) p.set('tab', ui.tab);
  if (ui.inst) p.set('inst', ui.inst);
  if (ui.out) p.set('out', ui.out);
  if (ui.trace) p.set('trace', ui.trace);
  return p;
}
const q = (p) => (p.size ? `?${p}` : '');
/** The exact link for an execution and selection: what Copy link copies. It never carries where you came from. */
export const execLink = () => `#/x/${ui.x}${q(selParams(new URLSearchParams()))}`;
/** A link that opens an execution page and returns to `back` (by default, the page you are on). */
export function execHref(id, { sel, cite, back = ui.here } = {}) {
  const p = new URLSearchParams();
  if (sel) p.set('sel', sel);
  if (cite) p.set('cite', cite);
  if (back) p.set('back', back);
  return `#/x/${id}${q(p)}`;
}
export const verifierHref = (key, tab) => `#/cards/verifiers/${key}${tab === 'yaml' ? '?tab=yaml' : ''}`;
export const pullHref = (card, pr, set) => `#/repositories/${card}/pulls/${pr}${set ? `?set=${set}` : ''}`;
export const cardOfRepo = (full) => REPOS.find((r) => r.full === full)?.card;

export function verifyParams() {
  const p = new URLSearchParams();
  for (const [k, v] of ui.f) p.append(k, v);
  if (ui.range !== '24h') p.set('range', ui.range);
  if (ui.brush) { p.set('t0', ui.brush[0]); p.set('t1', ui.brush[1]); }
  if (ui.group !== 'none') p.set('group', ui.group);
  return p;
}
export const verifyHref = (filters = [], range) => {
  const p = new URLSearchParams();
  for (const [k, v] of filters) p.append(k, v);
  if (range) p.set('range', range);
  return `#/verify${q(p)}`;
};

function repoParams({ v = ui.v, dtab = ui.dtab, reviews = ui.reviews } = {}) {
  const r = REPOS.find((x) => x.card === ui.card);
  const p = new URLSearchParams();
  if (v && v !== r?.selected) p.set('v', v);
  if (dtab !== 'yaml') p.set('tab', dtab);
  if (reviews !== 'latest') p.set('reviews', reviews);
  return p;
}
export const repoHref = (o = {}) => `#/repositories/${ui.card}${q(repoParams(o))}`;
/** The Repositories list with its last search and Show filter, so breadcrumbs return to the same list. */
export function reposHref() {
  const p = new URLSearchParams();
  if (ui.q) p.set('q', ui.q);
  if (ui.show !== 'all') p.set('show', ui.show);
  return `#/repositories${q(p)}`;
}

function hashFor() {
  switch (ui.route) {
    case 'exec': {
      const p = selParams(new URLSearchParams());
      if (ui.cite) p.set('cite', ui.cite);
      if (ui.back) p.set('back', ui.back);
      return `#/x/${ui.x}${q(p)}`;
    }
    case 'verify': return `#/verify${q(verifyParams())}`;
    case 'verifier': {
      const p = new URLSearchParams();
      if (ui.vtab === 'yaml') p.set('tab', 'yaml');
      else if (ui.sel) p.set('sel', ui.sel);
      return `#/cards/verifiers/${ui.vdef}${q(p)}`;
    }
    case 'repos': return reposHref();
    case 'repo': return `#/repositories/${ui.card}${q(repoParams())}`;
    default: return location.hash;
  }
}
/** Mirrors in-page state into the hash without adding a history entry. */
export function sync() {
  const h = hashFor();
  if (location.hash !== h) history.replaceState(null, '', h);
  ui.here = h || '#/verify';
}
/** Navigates to another page: a new history entry, then the page restores itself from the hash. */
export function go(hash) {
  if (ui.route === 'verify') ui.scroll = scrollY;
  location.hash = hash;
}

/* ---------- Verify ---------- */

export const hasFilter = (k, v) => ui.f.some(([f, x]) => f === k && x === v);
export function toggleFilter(k, v) {
  ui.f = hasFilter(k, v) ? ui.f.filter(([f, x]) => !(f === k && x === v)) : [...ui.f, [k, v]];
  ui.shown = 50;
  sync();
}
export function setRange(r) { ui.range = r; ui.brush = null; ui.shown = 50; sync(); }
export function setBrush(from, to) { ui.brush = [from, to]; ui.shown = 50; sync(); }
/** The toolbar's − and + zoom the time window about its centre, within the chosen range. */
export function zoomBy(k) {
  const [a, b] = timeWindow();
  const full = RANGES[ui.range][1];
  const span = Math.min(full, Math.max(15 * 60000, (b - a) * k));
  if (span >= full) { ui.brush = null; ui.shown = 50; return sync(); }
  const mid = Math.min(NOW - span / 2, (a + b) / 2);
  setBrush(Math.round(mid - span / 2), Math.round(mid + span / 2));
}
export function setGroup(g) { ui.group = g; sync(); }
export function setLive(on) { ui.live = on; if (on) ui.held = []; }

/** Applies the next simulated live event. While Live is paused, arrivals are held out of the list. */
export function nextEvent() {
  const e = EVENTS[ui.events];
  if (!e) return;
  const arrived = e.apply(execs, sets);
  if (Array.isArray(arrived) && !ui.live) ui.held = [...ui.held, ...arrived];
  ui.events += 1;
}

/* ---------- Decisions and cancellation (simulated server responses) ---------- */

const later = (fn) => setTimeout(fn, 450);
const stamp = () => clock(NOW + 7000);

/** Submits Approve/Reject for the exact occurrence. The preview strip picks the simulated server response. */
export function decide(id, step, decision, reason) {
  const x = execs[id];
  const s = x.steps[step];
  const a = s.approval;
  if (ui.busy || a.state !== 'pending') return;
  ui.busy = `${id}:decide`;
  const response = ui.respond;
  later(() => {
    ui.busy = '';
    if (x.steps[step].approval.state !== 'pending') {
      ui.notice[id] = { kind: 'warn', text: `Refreshed: ${a.occurrence} is ${a.state}. Your ${decision === 'approved' ? 'approval' : 'rejection'} was not applied.` };
      return;
    }
    if (response === 'accepted') {
      Object.assign(a, { state: decision, by: 'you (preview operator)', at: stamp(), reason: reason || null, receipt: 'dec-91d0e4a7' });
      s.action = 'succeeded';
      s.inst[0].state = 'succeeded';
      s.inst[0].outs.push(O('decision.json', 'json', { occurrence: a.occurrence, decision, reason: reason || null }));
      ui.notice[id] = { kind: 'ok', text: `Accepted by the server: ${decision} ${a.occurrence} (receipt ${a.receipt}). The same execution continues; no new work was started.` };
    } else if (response === 'conflict') {
      Object.assign(a, { state: 'rejected', by: 'r.okafor', at: clock(NOW - 20000), reason: 'Wait for the bias audit to finish.', receipt: 'dec-7e22b0c5' });
      s.action = 'succeeded';
      s.inst[0].state = 'succeeded';
      s.inst[0].outs.push(O('decision.json', 'json', { occurrence: a.occurrence, decision: 'rejected', reason: a.reason }));
      ui.notice[id] = { kind: 'warn', text: `Refreshed: r.okafor already rejected ${a.occurrence}. Your ${decision === 'approved' ? 'approval' : 'rejection'} was not applied.` };
    } else {
      Object.assign(a, { state: 'expired' });
      s.action = 'failed';
      s.inst[0].state = 'failed';
      s.inst[0].reason = `Approval occurrence ${a.occurrence} expired without a decision.`;
      ui.notice[id] = { kind: 'warn', text: `Refreshed: ${a.occurrence} has expired. Nothing was applied.` };
    }
  });
}

export const cancellable = (x) => ['pending', 'running'].includes(x.status);
/** Cancels unfinished work. Published outputs and judged requirements are kept; a result that wins the race is kept. */
export function cancel(id) {
  const x = execs[id];
  if (ui.busy || !cancellable(x)) return;
  ui.busy = `${id}:cancel`;
  const race = ui.race === 'finished' && id === 'exe-7f3a91c2';
  later(() => {
    ui.busy = '';
    if (race) {
      finishRunning(x);
      ui.notice[id] = { kind: 'warn', text: 'The execution completed before the cancellation was applied. Its recorded result is kept.' };
      return;
    }
    cancelled(x);
    ui.notice[id] = { kind: 'info', text: 'Cancelled. Published outputs and judged requirements are kept; provider calls already sent are not retracted.' };
  });
}
function cancelled(x) {
  x.status = 'cancelled';
  for (const s of Object.values(x.steps)) {
    for (const i of s.inst) if (['started', 'awaiting_approval'].includes(i.state)) { i.state = 'cancelled'; i.reason = 'Cancelled.'; }
    if (['running', 'waiting', 'awaiting_input', 'awaiting_approval'].includes(s.action)) s.action = 'cancelled';
    if (s.approval?.state === 'pending') s.approval.state = 'cancelled';
  }
}

/* ---------- Repositories and the GitHub connection ---------- */

export const connected = () => ui.conn === 'connected';
export const admin = () => ui.role === 'admin';
// Each fact answers one question; none implies another.
export function access(r) {
  if (ui.conn === 'disconnected' || ui.conn === 'handoff') return { key: 'none', glyph: '○', cls: 'c-muted', word: 'No connection', why: 'GitHub is not connected for this tenant.' };
  if (ui.conn === 'lost') return { key: 'lost', glyph: '✕', cls: 'c-danger', word: 'Lost', why: 'The GitHub installation is suspended or removed.' };
  return r.access
    ? { key: 'granted', glyph: '✓', cls: 'c-ok', word: 'Granted', why: 'Installation 41822907 can read ' + r.full + '.' }
    : { key: 'removed', glyph: '✕', cls: 'c-danger', word: 'Removed', why: `${r.full} is no longer selected for the installation on GitHub.` };
}
export function readiness(r) {
  if (!r.card) return { key: 'undeclared', glyph: '○', cls: 'c-muted', word: 'Not declared', why: 'No Repository Card is registered for this repository.' };
  const n = r.versions[r.selected].verifiers.length;
  if (access(r).key !== 'granted') return { key: 'inactive', glyph: '⊘', cls: 'c-warn', word: 'Inactive', why: `No new executions start without GitHub access. ${n} binding${n === 1 ? '' : 's'} stay declared.` };
  if (!n) return { key: 'none', glyph: '○', cls: 'c-muted', word: 'Not configured', why: `Selected version ${r.selected} has verifier = [].` };
  return { key: 'ready', glyph: '●', cls: 'c-ok', word: 'Ready', why: `${n} Verifier${n === 1 ? '' : 's'} run on pull_request.` };
}
/** Review sets of one Repository Card, newest first. */
export const setsOf = (card, pr) => sets.filter((s) => !s.hidden && s.card === card && (pr == null || s.pr === pr)).sort((a, b) => b.at - a.at);

/** Disconnecting stops new verification and cancels unfinished Repository executions; recorded evidence stays. */
export function disconnect(state = 'disconnected') {
  ui.conn = state;
  for (const x of Object.values(execs)) if (x.repo && cancellable(x)) cancelled(x);
}
/** Preview control: a lost or removed connection cancels unfinished Repository work exactly like a disconnect. */
export function setConn(state) {
  if (state === 'connected') ui.conn = state;
  else disconnect(state);
}
export function connect() {
  ui.conn = 'handoff';
  setTimeout(() => { if (ui.conn === 'handoff') ui.conn = 'connected'; }, 900);
}

/* ---------- Routing ---------- */

function applyVerify(p) {
  ui.f = FIELDS.flatMap((k) => p.getAll(k).map((v) => [k, v]));
  ui.range = p.get('range') in RANGES ? p.get('range') : '24h';
  const t0 = Number(p.get('t0')), t1 = Number(p.get('t1'));
  ui.brush = t0 && t1 > t0 ? [t0, t1] : null;
  ui.group = ['run', 'card', 'service', 'session', 'repo', 'pr'].includes(p.get('group')) ? p.get('group') : 'none';
}
function applySel(p) {
  const { def, exec } = current();
  const sel = p.get('sel') ?? '';
  if (!def || !valid(sel, def, exec) || (exec && !sel)) return select(exec ? focusOf(exec) : '');
  select(sel, { inst: p.get('inst') ?? undefined, tab: p.get('tab') ?? undefined, out: p.get('out') ?? undefined });
  if (p.get('trace') && exec?.spans?.some((s) => s.id === p.get('trace'))) openTrace(p.get('trace'));
}
function missing(what) { ui.route = 'missing'; ui.missing = what; }

/** Restores the page from the hash. Every page of the mock is one route of this shell. */
export function fromHash() {
  const was = ui.route;
  const [path, qs = ''] = (location.hash.slice(1) || '/verify').split('?');
  const p = new URLSearchParams(qs);
  const parts = path.split('/').filter(Boolean).map(decodeURIComponent);
  ui.sel = '';
  if (parts[0] === 'x') {
    if (!execs[parts[1]] || execs[parts[1]].hidden) return missing(`execution ${parts[1]}`);
    ui.route = 'exec';
    ui.x = parts[1];
    ui.last = parts[1];
    ui.back = p.get('back') ?? '';
    ui.cite = p.get('cite') ?? '';
    return applySel(p);
  }
  if (parts[0] === 'cards' && parts[1] === 'verifiers' && parts[2]) {
    if (!DEFS[parts[2]]) return missing(`Verifier ${parts[2]}`);
    ui.route = 'verifier';
    ui.vdef = parts[2];
    ui.vtab = p.get('tab') === 'yaml' ? 'yaml' : 'graph';
    return applySel(p);
  }
  if (parts[0] === 'repositories') {
    if (parts[1] === 'github') {
      if (!REPOS.some((r) => !r.card && r.id === parts[2])) return missing(`repository ${parts[2]}`);
      ui.route = 'undeclared'; ui.ghid = parts[2]; return;
    }
    if (parts[1]) {
      const r = REPOS.find((x) => x.card === parts[1]);
      if (!r) return missing(`Repository Card ${parts[1]}`);
      ui.card = r.card;
      if (parts[2] === 'pulls') {
        ui.pr = Number(parts[3]);
        if (!setsOf(r.card, ui.pr).length) return missing(`pull request ${r.full}#${parts[3]}`);
        ui.route = 'pull';
        ui.set = p.get('set') ?? '';
        if (ui.set && !setsOf(r.card, ui.pr).some((s) => s.id === ui.set)) return missing(`review set ${ui.set}`);
        return;
      }
      ui.route = 'repo';
      ui.v = r.versions[p.get('v')] ? p.get('v') : r.selected;
      ui.dtab = p.get('tab') === 'toml' ? 'toml' : 'yaml';
      ui.reviews = p.get('reviews') === 'all' ? 'all' : 'latest';
      return;
    }
    ui.route = 'repos';
    ui.q = p.get('q') ?? '';
    ui.show = ['ready', 'inactive', 'undeclared'].includes(p.get('show')) ? p.get('show') : 'all';
    return;
  }
  if (parts[0] === 'settings') { ui.route = 'settings'; return; }
  if (parts[0] !== 'verify') return missing(location.hash);
  ui.route = 'verify';
  applyVerify(p);
  if (was !== 'exec') ui.shown = 50;
}

/** Hash navigation: restore the page, then return to the list's scroll position or start a new page at the top. */
export function onHash() {
  const was = ui.route;
  fromHash();
  ui.here = location.hash || '#/verify';
  if (ui.route === 'verify' && was === 'exec') tick().then(() => scrollTo(0, ui.scroll));
  else scrollTo(0, 0);
}
