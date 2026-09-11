<script lang="ts">
  // S-02/SM-02 — the deterministic linked-Card graph (REQ-113) as the design
  // renders it: four ordered lanes of node cards with kind pills and
  // tone-colored statuses, drawn labeled relationship wires between nodes, a
  // dashed spine binding the Service declaration to its runtime components,
  // and a selected-node drawer that keeps the graph in view. Wires are
  // measured from the real DOM after layout and hidden when the lanes stack,
  // where left-to-right direction no longer exists. Selection is URL state.
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import type { CardDetail, ServiceNode, ServicePresentation } from '../../core/types';
  import { serviceHref, type ServiceScope } from './service-state';

  let {
    spec,
    detail,
    base,
    scope,
    path
  }: {
    spec: ServicePresentation;
    detail: CardDetail;
    base: string;
    scope: ServiceScope;
    path: string;
  } = $props();

  const composition = $derived(spec.composition);
  /** The node the URL selects, if it names one in the projected graph. */
  const selected = $derived<ServiceNode | undefined>(
    composition.lanes.flatMap((lane) => lane.nodes).find((node) => node.uid === scope.sel)
  );
  /** Link that selects `uid` — or clears the selection for an empty uid. */
  function selHref(uid: string): string {
    return serviceHref(path, detail.version, scope, { sel: uid });
  }

  /** Status tone from the server-projected glyph — the browser reads, never judges. */
  function tone(status: string): 'ok' | 'danger' | 'neutral' {
    return status.startsWith('✓') ? 'ok' : status.startsWith('✕') ? 'danger' : 'neutral';
  }

  /** One drawn wire: an SVG path plus its label anchor. */
  type Wire = { d: string; dash?: boolean; label?: string; lx: number; ly: number; anchor: string };

  let graphEl = $state<HTMLElement | null>(null);
  let wires = $state<Wire[]>([]);
  let wireH = $state(0);

  /**
   * Measure every `[data-uid]` node relative to the graph container and route
   * the declared edges: lane-to-lane elbows at the nodes' vertical centers,
   * `down` wires inside a lane, `under` wires through the channel below the
   * lanes, and the dashed Service spine along the runtime lane's left edge.
   * Runs on mount and on every container resize; empties when lanes stack.
   */
  function layout(): void {
    if (!graphEl) return;
    const box = graphEl.getBoundingClientRect();
    if (box.width < 700) {
      wires = [];
      wireH = 0;
      return;
    }
    type Box = { l: number; t: number; r: number; b: number; cx: number; cy: number };
    const pos = new Map<string, Box>();
    for (const el of graphEl.querySelectorAll<HTMLElement>('[data-uid]')) {
      const r = el.getBoundingClientRect();
      pos.set(el.dataset.uid ?? '', {
        l: r.left - box.left,
        t: r.top - box.top,
        r: r.right - box.left,
        b: r.bottom - box.top,
        cx: (r.left + r.right) / 2 - box.left,
        cy: (r.top + r.bottom) / 2 - box.top
      });
    }
    const next: Wire[] = [];
    let channel = Math.max(0, ...[...pos.values()].map((p) => p.b)) + 16;
    for (const edge of composition.edges) {
      const a = pos.get(edge.from);
      const b = pos.get(edge.to);
      if (!a || !b) continue;
      if (edge.route === 'down') {
        next.push({
          d: `M ${a.cx} ${a.b} V ${b.t - 1}`,
          label: edge.label,
          lx: a.cx + 6,
          ly: (a.b + b.t) / 2 + 3,
          anchor: 'start'
        });
      } else if (edge.route === 'under') {
        // Enter from below when the column under the target is clear; when a
        // node sits beneath it, rise just left of the target lane and enter
        // the left edge instead of cutting through that node.
        const blocked = [...pos.values()].some(
          (p) => p !== b && p.t >= b.b && p.l < b.r && p.r > b.l
        );
        const riseX = blocked ? b.l - 6 : b.cx;
        const rise = blocked ? `V ${b.b - 10} H ${b.l - 1}` : `V ${b.b + 1}`;
        next.push({
          d: `M ${a.cx} ${a.b} V ${channel} H ${riseX} ${rise}`,
          label: edge.label,
          lx: (a.cx + riseX) / 2,
          ly: channel - 4,
          anchor: 'middle'
        });
        channel += 16;
      } else {
        const mid = (a.r + b.l) / 2;
        next.push({
          d: `M ${a.r} ${a.cy} H ${mid} V ${b.cy} H ${b.l - 1}`,
          label: edge.label,
          lx: (a.r + mid) / 2 + (mid - a.r) / 2,
          ly: a.cy - 5,
          anchor: 'middle'
        });
      }
    }
    // The dashed spine: Service declaration down the runtime lane, one stub
    // per runtime component — declaration binding, not a runtime claim.
    const svc = pos.get('__service');
    const runtime = composition.lanes
      .find((lane) => lane.title === 'RUNTIME COMPOSITION')
      ?.nodes.map((n) => pos.get(n.uid))
      .filter((p): p is Box => !!p);
    if (svc && runtime?.length) {
      const spineX = Math.min(...runtime.map((p) => p.l)) - 10;
      const last = runtime[runtime.length - 1];
      next.push({ d: `M ${spineX} ${svc.b} V ${last.cy}`, dash: true, lx: 0, ly: 0, anchor: 'start' });
      for (const p of runtime) {
        next.push({ d: `M ${spineX} ${p.cy} H ${p.l - 1}`, dash: true, lx: 0, ly: 0, anchor: 'start' });
      }
    }
    wires = next;
    wireH = channel + 4;
  }

  $effect(() => {
    if (!graphEl) return;
    const observer = new ResizeObserver(() => layout());
    observer.observe(graphEl);
    layout();
    return () => observer.disconnect();
  });
</script>

{#each composition.intro as line (line)}
  <p class="muted">{line}</p>
{/each}
<Panel title="COMPOSITION — THE COMPLETE LINKED SERVICE" variant="quiet">
  {#snippet head()}<span class="mono muted">deterministic lanes · select a node</span>{/snippet}
  <div class="graph" bind:this={graphEl} style={wireH ? `padding-bottom:${Math.max(0, wireH - (graphEl?.scrollHeight ?? 0)) + 28}px` : undefined}>
    <svg class="wires" aria-hidden="true" style="height:100%">
      <defs>
        <marker id="svc-arrow" markerWidth="7" markerHeight="7" refX="5" refY="3.5" orient="auto">
          <path d="M 0 0 L 6 3.5 L 0 7 Z" />
        </marker>
      </defs>
      {#each wires as wire, i (i)}
        <path d={wire.d} marker-end={wire.dash ? undefined : 'url(#svc-arrow)'} stroke-dasharray={wire.dash ? '3 4' : undefined} />
        {#if wire.label}<text x={wire.lx} y={wire.ly} text-anchor={wire.anchor}>{wire.label}</text>{/if}
      {/each}
    </svg>
    <div class="lanes">
      {#each composition.lanes as lane (lane.title)}
        <div class="lane">
          <span class="lane-h">{lane.title}{#if lane.note} · {lane.note}{/if}</span>
          {#if lane.title === 'RUNTIME COMPOSITION'}
            <span class="svcbox mono" data-uid="__service">Service {detail.name} {detail.version}</span>
          {/if}
          {#each lane.nodes as node (node.uid + node.note)}
            <a
              class="node"
              data-uid={node.uid}
              href={selHref(node.uid)}
              aria-current={scope.sel === node.uid ? 'true' : undefined}
              data-sveltekit-noscroll
            >
              <span class="nrow"><span class="pill">● {node.kind.toUpperCase()}</span><span class="nn">{node.name}</span></span>
              <span class="st mono" data-tone={tone(node.status)}>{node.version} · {node.status}</span>
              <span class="mono muted">{node.note}</span>
            </a>
          {/each}
          {#if !lane.nodes.length && lane.note}
            <StateBlock state="absent" title="None declared" detail={lane.note} />
          {/if}
        </div>
      {/each}
    </div>
  </div>
  <p class="mono muted">{composition.aliasNote}</p>
</Panel>
{#if selected}
  <aside class="drawer" data-tone={tone(selected.status)} aria-label={`Selected — ${selected.name}`}>
    <div class="row between">
      <span class="mono"><strong>SELECTED — {selected.name.toUpperCase()} ({selected.kind.toUpperCase()} · {selected.version.toUpperCase()})</strong></span>
      <a class="mono muted" href={selHref('')} data-sveltekit-noscroll>✕ close · graph stays in view</a>
    </div>
    <div class="drawer-cols">
      <div class="stack">
        <span class="mono headline">{selected.drawer?.headline ?? `${selected.status} · ${selected.note}`}</span>
        <span class="row links">
          <a class="mono" href={`${base}/cards/${selected.uid}`}>Open Card → /cards/{selected.uid}</a>
          {#if selected.drawer?.observe}
            <a class="mono" href={base + selected.drawer.observe.href}>{selected.drawer.observe.label}</a>
          {/if}
        </span>
      </div>
      <div class="stack">
        {#if selected.drawer?.in}<span class="mono muted">{selected.drawer.in}</span>{/if}
        {#if selected.drawer?.out}<span class="mono muted">{selected.drawer.out}</span>{/if}
        <a class="mono muted" href={selHref('')} data-sveltekit-noscroll>‹ Back to Composition</a>
      </div>
    </div>
  </aside>
{/if}
<p class="mono muted">{composition.foot}</p>
