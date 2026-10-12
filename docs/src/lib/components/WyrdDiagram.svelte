<script lang="ts">
  // Nodes-and-edges diagram from a named spec in $lib/diagrams: component maps,
  // state machines, table joins, and pod topologies. It server-renders the full
  // static drawing; steps, scenarios, facets, and click-for-detail add focus and
  // moving packets in the browser, and motion stops under reduced motion.
  import { onDestroy } from 'svelte';
  import { diagram } from '$lib/diagrams';
  import { pathD, plain, pointAt, reducedMotion } from '$lib/diagrams/geometry';
  import type { DiagramEdge, DiagramNode, Row } from '$lib/diagrams/types';
  import Frame from './diagram/Frame.svelte';
  import Markers from './diagram/Markers.svelte';
  import NodeShape from './diagram/NodeShape.svelte';

  let { name }: { name: string } = $props();
  const spec = $derived(diagram(name, 'diagram'));
  const uid = $props.id();

  let viewIdx = $state(0);
  let step = $state<number | null>(null);
  let facet = $state<string | null>(null);
  let sel = $state<string | null>(null);
  let scenarioName = $state<string | null>(null);
  let say = $state<string | null>(null);
  let trail = $state<{ nodes: string[]; edges: string[] } | null>(null);
  let playing = $state(false);
  let loop = $state<DiagramEdge[]>([]);
  let shot = $state<{ e: DiagramEdge; f: number } | null>(null);
  let clock = $state(0);

  const view = $derived(spec.views[viewIdx]);
  const byId = $derived(new Map(view.nodes.map((n) => [n.id, n])));
  const edgeById = $derived(new Map(view.edges.map((e) => [e.id, e])));
  const hasDetail = $derived(
    !!spec.facets || view.nodes.some((n) => n.detail) || !!spec.steps?.some((s) => s.sql || s.note)
  );
  const focus = $derived(
    trail ?? (step !== null && spec.steps ? { nodes: spec.steps[step].nodes, edges: spec.steps[step].edges } : null)
  );

  const narration = $derived(
    say ??
      (step !== null && spec.steps ? spec.steps[step].caption : null) ??
      (facet ? spec.facets?.find((f) => f.id === facet)?.caption : null) ??
      view.rest
  );
  const selRows = $derived<Row[] | null>(sel ? (byId.get(sel)?.detail ?? null) : null);
  const facetRows = $derived(facet ? (spec.facets?.find((f) => f.id === facet)?.rows ?? null) : null);
  const stepDetail = $derived(step !== null && spec.steps ? spec.steps[step] : null);

  const text = $derived([
    ...spec.views.map((v) => v.rest),
    ...(spec.steps ?? []).map((s) => `<b>${s.label}.</b> ${s.caption}`),
    ...(spec.facets ?? []).map((f) => f.caption),
    ...spec.views.flatMap((v) =>
      v.nodes.filter((n) => n.detail).map((n) => `<b>${n.title}.</b> ${n.detail![n.detail!.length - 1][1]}`)
    )
  ]);

  // Packets: one per looping edge, staggered, plus the one-shot scenario hop.
  const packets = $derived.by(() => {
    const out: { x: number; y: number; kind: string; big?: boolean }[] = [];
    loop.forEach((e, i) => {
      const [x, y] = pointAt(e.pts, (clock / 1700 + i * 0.13) % 1);
      out.push({ x, y, kind: e.kind });
    });
    if (shot) {
      const [x, y] = pointAt(shot.e.pts, shot.f);
      out.push({ x, y, kind: shot.e.kind, big: true });
    }
    return out;
  });

  let raf = 0;
  $effect(() => {
    if (loop.length === 0) return;
    const t0 = performance.now();
    const tick = (now: number) => {
      clock = now - t0;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  let run = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const wait = (ms: number) => new Promise((r) => (timer = setTimeout(r, ms)));
  function hop(e: DiagramEdge, ms: number): Promise<void> {
    if (reducedMotion()) return Promise.resolve();
    return new Promise((done) => {
      const t0 = performance.now();
      const tick = (now: number) => {
        const f = Math.min(1, (now - t0) / ms);
        shot = { e, f };
        if (f < 1) requestAnimationFrame(tick);
        else {
          shot = null;
          done();
        }
      };
      requestAnimationFrame(tick);
    });
  }

  function reset() {
    run++;
    clearTimeout(timer);
    playing = false;
    step = null;
    facet = null;
    sel = null;
    scenarioName = null;
    say = null;
    trail = null;
    loop = [];
    shot = null;
  }
  function showStep(i: number) {
    const s = spec.steps![i];
    step = i;
    say = null;
    trail = null;
    loop = reducedMotion() ? [] : s.edges.map((id) => edgeById.get(id)!).filter(Boolean);
  }
  function pickStep(i: number) {
    reset();
    showStep(i);
  }
  async function playSteps() {
    if (playing) return reset();
    reset();
    playing = true;
    const my = run;
    for (let i = 0; run === my; i = (i + 1) % spec.steps!.length) {
      showStep(i);
      await wait(3600);
    }
  }
  function pickFacet(id: string) {
    const next = facet === id ? null : id;
    reset();
    facet = next;
  }
  async function playFacets() {
    if (playing) return reset();
    reset();
    playing = true;
    const my = run;
    for (let i = 0; run === my; i = (i + 1) % spec.facets!.length) {
      facet = spec.facets![i].id;
      await wait(3800);
    }
  }
  async function playScenario(name: string) {
    reset();
    const sc = spec.scenarios!.find((s) => s.name === name)!;
    scenarioName = name;
    const my = run;
    const nodes: string[] = [];
    const edges: string[] = [];
    for (let i = 0; i < sc.path.length; i++) {
      if (run !== my) return;
      nodes.push(sc.path[i]);
      trail = { nodes: [...nodes], edges: [...edges] };
      sel = sc.path[i];
      say = sc.say[i];
      await wait(reducedMotion() ? 900 : 1100);
      if (run !== my) return;
      if (i < sc.path.length - 1) {
        const e = view.edges.find((x) => x.from === sc.path[i] && x.to === sc.path[i + 1]);
        if (e) {
          edges.push(e.id);
          trail = { nodes: [...nodes], edges: [...edges] };
          await hop(e, 650);
        }
      }
    }
  }
  function selectNode(n: DiagramNode) {
    if (!n.detail) return;
    run++;
    scenarioName = null;
    say = null;
    trail = null;
    sel = n.id;
  }
  function setView(i: number) {
    reset();
    viewIdx = i;
  }
  onDestroy(() => {
    run++;
    clearTimeout(timer);
  });

  // Facet focus: a tagged node lights up when it carries the facet, dims when it
  // does not; a chip shares its pod's dimming.
  function facetClass(n: DiagramNode): string {
    if (!facet || !n.tags) return '';
    if (n.kind === 'chip') {
      const parentDim = n.parent && !byId.get(n.parent)?.tags?.includes(facet);
      return (n.tags.includes(facet) ? ' hl' : '') + (parentDim ? ' dim' : '');
    }
    return n.tags.includes(facet) ? ' hl' : ' dim';
  }
  const kw = (sql: string) =>
    sql
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/\b(SELECT|FROM|JOIN|ON|WHERE|AND|AS|UNION ALL)\b/g, '<span class="kw">$1</span>');
  const markerKind = (k: string) => (k === 'best' ? 'data' : k);
</script>

<Frame answers={spec.answers} {narration} narrLabel={spec.narrLabel} {text}>
  {#snippet controls()}
    {#if spec.views.length > 1}
      {#each spec.views as v, i (v.label)}
        <button type="button" aria-pressed={viewIdx === i} onclick={() => setView(i)}>{v.label}</button>
      {/each}
    {/if}
    {#if spec.steps}
      <button type="button" aria-pressed={step === null && !playing} onclick={reset}>All</button>
      {#each spec.steps as s, i (s.label)}
        <button type="button" aria-pressed={step === i} onclick={() => pickStep(i)}>{s.label}</button>
      {/each}
      <button type="button" class="wd-play" onclick={playSteps}>{playing ? '■ Stop' : '▶ Play'}</button>
    {/if}
    {#if spec.facets}
      {#each spec.facets as f (f.id)}
        <button type="button" aria-pressed={facet === f.id} onclick={() => pickFacet(f.id)}>{f.label}</button>
      {/each}
      <button type="button" class="wd-play" onclick={playFacets}>{playing ? '■ Stop' : '▶ Play'}</button>
    {/if}
    {#if spec.scenarios}
      {#each spec.scenarios as sc (sc.name)}
        <button type="button" aria-pressed={scenarioName === sc.name} onclick={() => playScenario(sc.name)}>▶ {sc.name}</button>
      {/each}
      <button type="button" onclick={reset}>Reset</button>
    {/if}
  {/snippet}

  {#snippet canvas()}
    <svg
      viewBox={`0 0 ${view.size[0]} ${view.size[1]}`}
      role="img"
      aria-label={plain(spec.answers)}
      class:wd-focus={!!focus}
      class:wd-facet={!!facet}
    >
      <Markers id={uid} />
      {#each view.groups ?? [] as g (g.label)}
        <rect x={g.x} y={g.y} width={g.w} height={g.h} rx="2" class="wd-group" />
        <text x={g.x + 10} y={g.y + 16} class="wd-t-group">{g.label}</text>
      {/each}
      {#each view.links ?? [] as l (l.d)}
        <path d={l.d} class="wd-peer" />
        {#each l.labels as t (t.text)}
          <text x={t.x} y={t.y} class="wd-t-label" text-anchor={t.anchor ?? 'middle'}>{t.text}</text>
        {/each}
      {/each}
      {#each view.edges as e (e.id)}
        {@const stepFor = spec.steps?.findIndex((s) => s.edgeFor === e.id) ?? -1}
        <g class={`wd-e wd-k-${e.kind}`} class:on={focus?.edges.includes(e.id)}>
          <path d={pathD(e.pts)} class="wd-line" marker-end={`url(#${uid}-${markerKind(e.kind)})`} />
          {#if e.label}
            <text x={e.lx} y={e.ly} class="wd-t-label" text-anchor={e.anchor ?? 'middle'}>{e.label}</text>
          {/if}
          {#if stepFor >= 0}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <path d={pathD(e.pts)} class="wd-hit" onclick={() => pickStep(stepFor)} />
          {/if}
        </g>
      {/each}
      {#each view.nodes as n (n.id)}
        {#if n.kind === 'pod' || n.kind === 'chip'}
          <g class={`wd-${n.kind}${n.off ? ' off' : ''}${facetClass(n)}`}>
            <NodeShape node={n} />
          </g>
        {:else if n.detail}
          <g
            class="wd-n clickable"
            class:on={focus?.nodes.includes(n.id)}
            class:sel={sel === n.id}
            role="button"
            tabindex="0"
            aria-label={n.title}
            onclick={() => selectNode(n)}
            onkeydown={(ev) => {
              if (ev.key === 'Enter' || ev.key === ' ') {
                ev.preventDefault();
                selectNode(n);
              }
            }}
          >
            <NodeShape node={n} />
          </g>
        {:else}
          <g class="wd-n" class:on={focus?.nodes.includes(n.id)}>
            <NodeShape node={n} />
          </g>
        {/if}
      {/each}
      {#each view.texts ?? [] as t, i (i)}
        <text x={t.x} y={t.y} class={`wd-t-${t.cls ?? 'label'}`} text-anchor={t.anchor ?? 'middle'}>{t.text}</text>
      {/each}
      {#each packets as p, i (i)}
        <rect
          x={p.x - (p.big ? 4.5 : 4)}
          y={p.y - (p.big ? 4.5 : 4)}
          width={p.big ? 9 : 8}
          height={p.big ? 9 : 8}
          rx="1"
          class={p.kind === 'control' ? 'wd-pk-control' : 'wd-pk-data'}
        />
      {/each}
    </svg>
  {/snippet}

  {#snippet after()}
    {#if hasDetail}
      <div class="wd-detail">
        {#if facetRows || selRows}
          {#each (facetRows ?? selRows)! as [k, v] (k)}
            <div class="wd-row"><span>{k}</span><span>{@html v}</span></div>
          {/each}
        {:else if stepDetail?.sql}
          <pre class="wd-sql">{@html kw(stepDetail.sql)}</pre>
        {:else if stepDetail?.note}
          <p class="wd-note">{@html stepDetail.note}</p>
        {:else}
          <div class="wd-row"><span>Select</span><span>{spec.detailHint}</span></div>
        {/if}
      </div>
    {/if}
  {/snippet}
</Frame>
