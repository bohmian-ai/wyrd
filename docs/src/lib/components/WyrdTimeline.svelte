<script lang="ts">
  // Timeline with lanes, stages, and a scrubber from a named spec in
  // $lib/diagrams. Dragging the handle or pressing Play moves a row token
  // between lanes; the readouts under the drawing say what is true at that
  // moment. Under reduced motion, Play steps between fixed stops instead.
  import { diagram } from '$lib/diagrams';
  import { plain, reducedMotion } from '$lib/diagrams/geometry';
  import Frame from './diagram/Frame.svelte';

  let { name }: { name: string } = $props();
  const spec = $derived(diagram(name, 'timeline'));
  const uid = $props.id();

  const X0 = 170;
  const X1 = 820;
  const laneY = (i: number) => 46 + 66 * i;
  const xOf = (v: number) => X0 + ((X1 - X0) * v) / 100;

  // Null until the reader moves the scrubber; the spec's start frame until then.
  let scrub = $state<number | null>(null);
  const v = $derived(scrub ?? spec.start);
  let playing = $state(false);

  const phaseIdx = $derived(spec.phases.findLastIndex((p) => v >= p.from));
  const phase = $derived(spec.phases[phaseIdx]);
  // The token glides between lanes across a short window around each phase start.
  const tokenY = $derived.by(() => {
    for (let i = 1; i < spec.phases.length; i++) {
      const p = spec.phases[i];
      const prev = spec.phases[i - 1];
      const a = p.from - (spec.move * 2) / 3;
      const b = p.from + spec.move / 3;
      if (p.lane !== prev.lane && v >= a && v < b) {
        return laneY(prev.lane) + ((laneY(p.lane) - laneY(prev.lane)) * (v - a)) / (b - a);
      }
      if (v < a) return laneY(prev.lane);
    }
    return laneY(spec.phases[spec.phases.length - 1].lane);
  });
  const tokenX = $derived(Math.max(X0 + 24, Math.min(X1 - 24, xOf(v))));
  const text = $derived(spec.phases.map((p, i) => `<b>${spec.stages[i]?.name ?? ''}.</b> ${p.caption}`));

  let raf = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function stop() {
    playing = false;
    cancelAnimationFrame(raf);
    clearTimeout(timer);
  }
  function play() {
    if (playing) return stop();
    playing = true;
    if (reducedMotion()) {
      let i = 0;
      const next = () => {
        if (!playing) return;
        scrub = spec.stops[i++];
        if (i < spec.stops.length) timer = setTimeout(next, 2200);
        else playing = false;
      };
      return next();
    }
    const t0 = performance.now();
    const tick = (now: number) => {
      scrub = Math.min(100, (now - t0) / 120);
      if (scrub < 100 && playing) raf = requestAnimationFrame(tick);
      else stop();
    };
    raf = requestAnimationFrame(tick);
  }
  // Effects never run during SSR, so the animation cleanup stays browser-only.
  $effect(() => () => stop());
</script>

<Frame answers={spec.answers} narration={phase.caption} {text}>
  {#snippet controls()}
    <button type="button" class="wd-play" onclick={play}>{playing ? '■ Stop' : '▶ Play'}</button>
  {/snippet}

  {#snippet canvas()}
    <svg viewBox="0 0 860 250" role="img" aria-label={plain(spec.answers)}>
      {#each spec.lanes as label, i (label)}
        <rect x="150" y={laneY(i) - 22} width="690" height="44" rx="2" class="wd-lane" />
        <text x="140" y={laneY(i) + 4} class="wd-t-lane" text-anchor="end">{label}</text>
      {/each}
      <line x1={X0} y1="214" x2={X1} y2="214" class="wd-axis" />
      {#each spec.stages as s (s.name)}
        <line x1={xOf(s.at)} y1="22" x2={xOf(s.at)} y2="218" class="wd-tick" stroke-dasharray="2 4" />
        <text x={xOf(s.at) + 6} y="234" class="wd-t-stage">{s.name}</text>
        <text x={xOf(s.at) + 6} y="247" class="wd-t-stagesub">{s.sub}</text>
      {/each}
      <line x1={xOf(v)} y1="18" x2={xOf(v)} y2="218" class="wd-cursor" />
      {#if phase.ghostLane !== undefined}
        <g transform={`translate(${tokenX} ${laneY(phase.ghostLane)})`}>
          <rect x="-20" y="-11" width="40" height="22" rx="2" class="wd-f-surface wd-s-text wd-s-dash" />
          <text y="4" class="wd-t-col" text-anchor="middle">row</text>
        </g>
      {/if}
      <g transform={`translate(${tokenX} ${tokenY})`}>
        <rect x="-20" y="-11" width="40" height="22" rx="2" class="wd-f-evidence" />
        <text y="4" class="wd-t-col wd-t-ink" text-anchor="middle">row</text>
      </g>
    </svg>
  {/snippet}

  {#snippet under()}
    <div class="wd-tl">
      <label for={`${uid}-scrub`}>Time</label>
      <input
        id={`${uid}-scrub`}
        type="range"
        min="0"
        max="100"
        step="1"
        value={v}
        oninput={(e) => {
          stop();
          scrub = Number(e.currentTarget.value);
        }}
        aria-valuetext={plain(phase.readouts[0].v)}
      />
    </div>
    <div class="wd-facts">
      {#each spec.readouts as k, i (k)}
        <div class="wd-fact">
          <span class="wd-k">{k}</span>
          <span class="v">{@html phase.readouts[i].v}</span>
          <span class="s">{phase.readouts[i].s}</span>
        </div>
      {/each}
    </div>
  {/snippet}
</Frame>
