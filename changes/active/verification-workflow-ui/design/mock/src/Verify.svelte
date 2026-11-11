<script>
  import Timeline from './Timeline.svelte';
  import { LIFECYCLE, VERDICT, ACTION, NOW, clock, when } from './data.js';
  import { ui, population, timeWindow, toggleFilter, hasFilter, setRange, setGroup, setLive, zoomBy, failedOf, blockedOf, valuesOf, RANGES, sync, execHref, verifierHref, pullHref, verifyHref } from './store.svelte.js';

  // Everything below describes one population: the executions in the time window that match every filter.
  const rows = $derived(population());
  const win = $derived(timeWindow());
  const N = 48;
  const buckets = $derived(Array.from({ length: N }, (_, k) => ({ from: win[0] + (k * (win[1] - win[0])) / N, to: win[0] + ((k + 1) * (win[1] - win[0])) / N })));
  const bucketOf = (t) => Math.min(N - 1, Math.floor(((t - win[0]) / (win[1] - win[0])) * N));
  const tally = (key) => { const c = buckets.map(() => ({})); for (const x of rows) { const b = c[bucketOf(x.at)]; b[key(x)] = (b[key(x)] ?? 0) + 1; } return c; };

  // Chart 1: recorded verdicts, plus a neutral outline for executions with none. That series is not a verdict.
  const VERDICTS = [{ key: 'passed', label: 'passed', cls: 's-pass' }, { key: 'failed', label: 'failed', cls: 's-fail' },
    { key: 'inconclusive', label: 'inconclusive', cls: 's-inc' }, { key: 'none', label: 'no verdict', title: 'no recorded verdict (queued, running, errored, timed out or cancelled)', cls: 's-none' }];
  const byVerdict = $derived(tally((x) => x.verdict ?? 'none'));
  // Chart 2: the same population by exact Verifier identity; the three largest get a series, the rest share one, so
  // the legend stays on one line.
  const top = $derived.by(() => {
    const n = {};
    for (const x of rows) n[x.def] = (n[x.def] ?? 0) + 1;
    return Object.keys(n).sort((a, b) => n[b] - n[a] || a.localeCompare(b));
  });
  const VCLS = ['s-v0', 's-v1', 's-v2'];
  const verifierSeries = $derived([...top.slice(0, 3).map((k, i) => ({ key: k, label: k.split('@')[0], title: k, cls: VCLS[i] })),
    ...(top.length > 3 ? [{ key: 'other', label: `${top.length - 3} more`, title: top.slice(3).join(', '), cls: 's-vo' }] : [])]);
  const byVerifier = $derived(tally((x) => (top.indexOf(x.def) < 3 ? x.def : 'other')));

  const FACETS = [['status', 'Status'], ['verdict', 'Verdict'], ['verifier', 'Verifier'], ['failed', 'Failed requirement'], ['service', 'Service'], ['card', 'Card'],
    ['run', 'Run'], ['session', 'Session'], ['repo', 'Repository'], ['pr', 'Pull request']];
  const label = (k, v) => (k === 'status' ? LIFECYCLE[v][1] : k === 'verdict' && v === 'none' ? 'no recorded verdict' : v === 'none' ? 'not set' : v);
  const tone = (k, v) => ((k === 'verdict' && v === 'failed') || (k === 'status' && ['errored', 'timed_out'].includes(v)) ? 'c-danger' : v === 'none' || (k === 'verdict' && v === 'inconclusive') ? 'c-muted' : '');
  function facet(k) {
    const n = new Map();
    for (const x of rows) for (const v of valuesOf(x, k)) n.set(v, (n.get(v) ?? 0) + 1);
    for (const [f, v] of ui.f) if (f === k && !n.has(v)) n.set(v, 0);
    return [...n].sort((a, b) => b[1] - a[1] || String(a[0]).localeCompare(String(b[0])));
  }
  let more = $state({});
  const prLink = (v) => { const [repo, pr] = v.split('#'); return pullHref(repo.split('/')[1], pr); };

  // A read-only projection of the typed filters. It is not an editable query.
  const where = $derived.by(() => {
    const parts = FACETS.map(([k]) => {
      const vs = ui.f.filter(([f]) => f === k).map(([, v]) => v);
      if (!vs.length) return null;
      if (k === 'failed') return vs.map((v) => `'${v}' IN failed_requirements`).join(' OR ');
      const nul = vs.includes('none') ? `${k} IS NULL` : null;
      const vals = vs.filter((v) => v !== 'none');
      const eq = vals.length === 1 ? `${k} = '${vals[0]}'` : vals.length ? `${k} IN (${vals.map((v) => `'${v}'`).join(', ')})` : null;
      return [eq, nul].filter(Boolean).join(' OR ');
    }).filter(Boolean).map((p) => (p.includes(' OR ') ? `(${p})` : p));
    return parts.join(' AND ');
  });

  const GROUPS = [['none', 'none'], ['run', 'run'], ['card', 'Card'], ['service', 'Service'], ['session', 'session'], ['repo', 'repository'], ['pr', 'pull request']];
  const groupName = { run: 'run', card: 'Card', service: 'Service', session: 'session', repo: 'repository', pr: 'pull request' };
  // Paging is honest: the first `shown` rows of the one population, newest first, then grouped.
  const visible = $derived(rows.slice(0, ui.shown));
  const groups = $derived.by(() => {
    if (ui.group === 'none') return [{ key: '', head: null, rows: visible }];
    const m = new Map();
    for (const x of visible) { const k = valuesOf(x, ui.group)[0]; if (!m.has(k)) m.set(k, []); m.get(k).push(x); }
    const list = [...m].filter(([k]) => k !== 'none').map(([key, rs]) => {
      // A session group is headed by its recorded session judgment, if the population contains it. Grouping never computes one.
      const head = ui.group === 'session' ? rs.find((x) => x.def.startsWith('session-resolution@')) ?? null : null;
      return { key, head, rows: rs.filter((x) => x !== head) };
    });
    if (m.has('none')) list.push({ key: 'none', head: null, rows: m.get('none') });
    return list;
  });
  let collapsed = $state({});
  const summary = (rs) => {
    const n = {};
    for (const x of rs) n[x.verdict ?? 'none'] = (n[x.verdict ?? 'none'] ?? 0) + 1;
    return [`${rs.length} ${rs.length === 1 ? 'execution' : 'executions'}`, ...VERDICTS.filter((v) => n[v.key]).map((v) => `${n[v.key]} ${v.key === 'none' ? 'no verdict' : v.label}`)].join(' · ');
  };
  const fmtTime = (t) => (NOW - t < 86400000 ? clock(t) : when(t));
  const remember = () => { ui.scroll = scrollY; };
  // Matches the `.vt` min-width in app.css: below it the table scrolls sideways and the foot says so.
  const TABLE_MIN = 1184;
  let tableW = $state(0);
</script>

{#snippet row(x)}
  {@const life = LIFECYCLE[x.status]}
  {@const failed = failedOf(x)}
  <a class="vt" data-row={x.id} href={execHref(x.id)} aria-current={ui.last === x.id ? 'true' : undefined} onclick={remember}
    aria-label="{x.def} {life[1]}{x.verdict ? `, ${x.verdict}` : ''}, {fmtTime(x.at)}">
    <span class="muted">{fmtTime(x.at)}</span>
    <span title={x.def}>{x.def.split('@')[0]}</span>
    <span class="muted" title={x.card}>{x.card}</span>
    <span class="muted">{x.run ?? '—'}</span>
    <span class="muted">{x.session ?? '—'}</span>
    <span class="muted" title={x.service ?? ''} data-service>{x.service ?? '—'}</span>
    <span class="muted" title={x.repo ? `${x.repo}#${x.pr} · head ${x.head}` : ''}>{#if x.repo}#{x.pr} <span>{x.repo.split('/')[1]}</span>{:else}—{/if}</span>
    <span class="out">
      <span class="st {life[2]}"><span aria-hidden="true">{life[0]}</span>{life[1]}{#each blockedOf(x) as b (b)}<span class="c-warn" title={ACTION[b][1]} data-blocked={b}> ◷</span>{/each}</span>
      <span class="fails" title={failed.join(', ')}>{failed.join(', ')}</span>
      <span class="vd">{#if x.verdict}{@const v = VERDICT[x.verdict]}<span class="badge {v[2]}"><span aria-hidden="true">{v[0]}</span>{v[1]}</span>{:else}<span class="muted small">none</span>{/if}</span>
    </span>
  </a>
{/snippet}

<div class="dash" data-testid="dashboard">
  <section class="qbar" aria-label="Query">
    <div class="where-wrap" title="Read-only: the WHERE clause the selected filters describe">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true" class="muted"><circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/></svg>
      <span class="mono small muted">WHERE</span>
      <output class="where mono" data-testid="where" aria-label="WHERE: read-only projection of the active filters">{#if where}{where}{:else}<span class="muted">select facet values to filter</span>{/if}</output>
      {#if ui.f.length}<button class="linkbtn small" data-testid="clear" onclick={() => { ui.f = []; ui.shown = 50; sync(); }}>Clear</button>{/if}
    </div>
    <div class="zoom" role="group" aria-label="Zoom">
      <button class="btn" aria-label="Zoom out" data-testid="zoom-out" onclick={() => zoomBy(2)}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M5 12h14"/></svg></button>
      <button class="btn" aria-label="Zoom in" data-testid="zoom-in" onclick={() => zoomBy(0.5)}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M5 12h14M12 5v14"/></svg></button>
    </div>
    <label class="range btn"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="3" y="5" width="18" height="16"/><path d="M3 10h18M8 3v4M16 3v4"/></svg>
      <span class="sr">Time range</span>
      <select data-testid="range" value={ui.range} onchange={(e) => setRange(e.currentTarget.value)}>
        {#each Object.entries(RANGES) as [k, [text]] (k)}<option value={k}>{text}</option>{/each}
      </select>
    </label>
    <button class="btn live" data-testid="live" aria-pressed={ui.live} title="While paused, newly arriving executions are held out of the list" onclick={() => setLive(!ui.live)}>
      <i class="dot" class:on={ui.live} aria-hidden="true"></i>{ui.live ? 'Live' : 'Paused'}
    </button>
  </section>
  {#if ui.held.length}
    <p class="banner info" role="status" data-testid="held">{ui.held.length} new {ui.held.length === 1 ? 'execution' : 'executions'} held while Live is paused. <button class="btn btn-sm" onclick={() => setLive(true)}>Resume Live</button></p>
  {/if}

  <section class="panel charts" aria-label="Timelines">
    <div class="charts-head">
      <span class="h">Drag across either chart to zoom both</span>
      {#if ui.brush}
        <span class="small mono" data-testid="zoomed">{when(win[0])} → {when(win[1])} UTC</span>
        <button class="linkbtn small" data-testid="reset-zoom" onclick={() => { ui.brush = null; ui.shown = 50; sync(); }}>Reset zoom</button>
      {/if}
    </div>
    <div class="charts-grid">
      <Timeline title="Executions" testid="chart-verdict" series={VERDICTS} counts={byVerdict} {buckets} />
      <Timeline title="By Verifier" testid="chart-verifier" series={verifierSeries} counts={byVerifier} {buckets} />
    </div>
  </section>

  <div class="grouping">
    <span class="small muted">Group by</span>
    <div class="segs" role="group" aria-label="Group by">
      {#each GROUPS as [k, text] (k)}<button class="seg" data-group={k} aria-pressed={ui.group === k} onclick={() => setGroup(k)}>{text}</button>{/each}
    </div>
    <span class="grow"></span>
    <span class="small mono muted" data-testid="count">{rows.length.toLocaleString('en-US')} {rows.length === 1 ? 'execution' : 'executions'}</span>
  </div>

  <div class="dash-body">
    <aside class="facets" aria-label="Filters" data-testid="facets">
      <span class="small muted">Click a value to add it to WHERE</span>
      {#each FACETS as [k, title] (k)}
        {@const vs = facet(k)}
        {#if vs.length}
          <section class="facet" data-facet={k}>
            <span class="h">{title}</span>
            {#each (more[k] ? vs : vs.slice(0, 4)) as [v, n] (v)}
              <div class="fv">
                <button class="fbtn" data-filter="{k}={v}" aria-pressed={hasFilter(k, v)} onclick={() => toggleFilter(k, v)}>
                  <span class="fname {tone(k, v)}">{#if hasFilter(k, v)}<span aria-hidden="true">✓ </span>{/if}{label(k, v)}</span><span class="fn mono">{n.toLocaleString('en-US')}</span>
                </button>
                {#if k === 'verifier'}<a class="def-link" href={verifierHref(v)} aria-label="Open Verifier {v}" title="Open the exact Verifier definition">↗</a>{/if}
                {#if k === 'pr' && v !== 'none'}<a class="def-link" href={prLink(v)} aria-label="Open pull request {v}" title="Open the pull request's review sets">↗</a>{/if}
              </div>
            {/each}
            {#if vs.length > 4}<button class="more mono small" onclick={() => (more[k] = !more[k])}>{more[k] ? '− fewer' : `+ ${vs.length - 4} more`}</button>{/if}
          </section>
        {/if}
      {/each}
    </aside>

    <section class="panel results" aria-label="Executions" data-testid="rows">
      <!-- Dense rows keep readable column widths and scroll sideways inside this region; the outcome columns stay pinned. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="vt-scroll" role="region" aria-label="Executions table" tabindex="0" bind:clientWidth={tableW}>
        <div class="vt vt-head h" aria-hidden="true"><span>Time ↓</span><span>Verifier</span><span>Card</span><span>Run</span><span>Session</span><span>Service</span><span>Repo · PR</span><span class="out"><span>Status</span><span>Failed</span><span class="vd">Verdict</span></span></div>
        {#each groups as g (g.key)}
          {#if ui.group !== 'none'}
            <div class="ghead" data-ghead={g.key}>
              <button class="gtoggle" aria-expanded={!collapsed[g.key]} aria-label="{collapsed[g.key] ? 'Expand' : 'Collapse'} {g.key}" onclick={() => (collapsed[g.key] = !collapsed[g.key])}>{collapsed[g.key] ? '▸' : '▾'}</button>
              {#if g.key === 'none'}
                <b>Ungrouped</b><span class="muted">no {groupName[ui.group]}{ui.group === 'session' ? ' · no session result' : ''}</span>
              {:else}
                <b class="mono">{g.key}</b>
                {#if ui.group === 'pr'}<a class="small" href={prLink(g.key)} data-pr-head={g.key}>review sets ↗</a>{/if}
                {#if ui.group === 'session'}
                  {#if g.head}<span class="small muted">session result in first row</span>{:else}<span class="small muted">no session result in this window</span>{/if}
                  <a class="small" href={verifyHref([['session', g.key]])}>filter</a>
                {/if}
              {/if}
              <span class="muted small push">{summary(g.head ? [g.head, ...g.rows] : g.rows)}</span>
            </div>
          {/if}
          {#if !collapsed[g.key]}
            {#if g.head}{@render row(g.head)}{/if}
            {#each g.rows as x (x.id)}{@render row(x)}{/each}
          {/if}
        {/each}
        {#if !rows.length}<p class="note" style="padding: 14px">No execution matches these filters in this window.</p>{/if}
      </div>
      <div class="foot mono small muted" data-testid="foot">
        <span>Showing {visible.length.toLocaleString('en-US')} of {rows.length.toLocaleString('en-US')} · newest first{#if tableW && tableW < TABLE_MIN} · scroll the table sideways for every column{/if}</span>
        {#if visible.length < rows.length}<button class="linkbtn small" data-testid="show-more" onclick={() => (ui.shown += 50)}>Show {Math.min(50, rows.length - visible.length)} more</button>{/if}
      </div>
    </section>
  </div>
</div>
