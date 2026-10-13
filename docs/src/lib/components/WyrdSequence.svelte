<script lang="ts">
  // Sequence diagram from a named spec in $lib/diagrams: participants with
  // lifelines, ordered messages, and tabs for alternative flows over the same
  // participants. Play reveals one message at a time with a moving packet.
  import { onDestroy } from 'svelte';
  import { diagram } from '$lib/diagrams';
  import { pathD, plain, pointAt, reducedMotion } from '$lib/diagrams/geometry';
  import type { Pt } from '$lib/diagrams/types';
  import Frame from './diagram/Frame.svelte';
  import Markers from './diagram/Markers.svelte';
  import NodeShape from './diagram/NodeShape.svelte';

  let { name }: { name: string } = $props();
  const spec = $derived(diagram(name, 'sequence'));
  const uid = $props.id();

  let flowIdx = $state(0);
  let shown = $state(-1);
  let playing = $state(false);
  let shot = $state<{ pts: Pt[]; kind: string; f: number } | null>(null);

  const flow = $derived(spec.flows[flowIdx]);
  const xOf = $derived(new Map(spec.participants.map((p) => [p.id, p.x])));
  const height = $derived(96 + flow.messages.length * 46 + 20);
  const msgs = $derived(
    flow.messages.map((m, i) => {
      const y = 96 + i * 46;
      const x1 = xOf.get(m.from)!;
      const x2 = xOf.get(m.to)!;
      if (m.from === m.to) {
        const pts: Pt[] = [[x1, y - 8], [x1 + 46, y - 8], [x1 + 46, y + 10], [x1 + 6, y + 10]];
        return { ...m, pts, lx: x1 + 54, ly: y + 4, anchor: 'start' as const, y };
      }
      const dir = Math.sign(x2 - x1);
      const pts: Pt[] = [[x1 + dir * 4, y], [x2 - dir * 6, y]];
      return { ...m, pts, lx: (x1 + x2) / 2, ly: y - 7, anchor: 'middle' as const, y };
    })
  );
  const narration = $derived(shown >= 0 ? msgs[shown].say : flow.rest);
  const text = $derived(spec.flows.flatMap((f) => [`<b>${f.name}.</b> ${f.rest}`, ...f.messages.map((m) => m.say)]));

  let run = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const wait = (ms: number) => new Promise((r) => (timer = setTimeout(r, ms)));
  function hop(pts: Pt[], kind: string, ms: number): Promise<void> {
    if (reducedMotion()) return Promise.resolve();
    return new Promise((done) => {
      const t0 = performance.now();
      const tick = (now: number) => {
        const f = Math.min(1, (now - t0) / ms);
        shot = { pts, kind, f };
        if (f < 1) requestAnimationFrame(tick);
        else {
          shot = null;
          done();
        }
      };
      requestAnimationFrame(tick);
    });
  }
  function stop() {
    run++;
    clearTimeout(timer);
    playing = false;
    shown = -1;
    shot = null;
  }
  function pickFlow(i: number) {
    stop();
    flowIdx = i;
  }
  async function play() {
    if (playing) return stop();
    stop();
    playing = true;
    const my = run;
    for (let i = 0; i < msgs.length; i++) {
      if (run !== my) return;
      shown = i;
      await hop(msgs[i].pts, msgs[i].kind === 'control' ? 'control' : 'data', 900);
      await wait(1500);
    }
    if (run === my) playing = false;
  }
  onDestroy(() => {
    run++;
    clearTimeout(timer);
  });
  const markerKind = (k: string) => (k === 'best' ? 'data' : k);
</script>

<Frame answers={spec.answers} {narration} {text}>
  {#snippet controls()}
    {#each spec.flows as f, i (f.name)}
      <button type="button" aria-pressed={flowIdx === i} onclick={() => pickFlow(i)}>{f.name}</button>
    {/each}
    <button type="button" class="wd-play" onclick={play}>{playing ? '■ Stop' : '▶ Play'}</button>
  {/snippet}

  {#snippet canvas()}
    <svg viewBox={`0 0 ${spec.width} ${height}`} role="img" aria-label={plain(flow.name + ': ' + spec.answers)} class:wd-focus={shown >= 0}>
      <Markers id={uid} />
      {#each spec.participants as p (p.id)}
        <line x1={p.x} y1="64" x2={p.x} y2={height - 10} class="wd-lifeline" />
        <!-- Participants stay readable while messages are focused. -->
        <g class="wd-n on">
          <NodeShape node={{ ...p, x: p.x - 80, y: 10, w: 160, h: 50 }} />
        </g>
      {/each}
      {#each msgs as m, i (i)}
        <g class={`wd-e wd-k-${m.kind}`} class:on={i <= shown}>
          <path d={pathD(m.pts)} class="wd-line" marker-end={`url(#${uid}-${markerKind(m.kind)})`} />
          <text x={m.lx} y={m.ly} class="wd-t-label" text-anchor={m.anchor}>{m.label}</text>
          {#if m.mark}
            <text x={xOf.get(m.mark.at)} y={m.y + 4} class="wd-t-x" text-anchor="middle">{m.mark.text}</text>
          {/if}
        </g>
      {/each}
      {#if shot}
        {@const [x, y] = pointAt(shot.pts, shot.f)}
        <rect x={x - 4.5} y={y - 4.5} width="9" height="9" rx="1" class={shot.kind === 'control' ? 'wd-pk-control' : 'wd-pk-data'} />
      {/if}
    </svg>
  {/snippet}
</Frame>
