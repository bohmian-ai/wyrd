<script>
  import { ui, setBrush } from './store.svelte.js';
  import { clock, day, NOW } from './data.js';

  // A compact stacked count histogram over the shared buckets. Dragging across either chart zooms both: the drag
  // range is shared through `ui.drag`, and the brushed window becomes the population's time filter.
  let { title, testid, series, counts, buckets } = $props();
  const totals = $derived(counts.map((c) => Object.values(c).reduce((a, b) => a + b, 0)));
  const max = $derived(Math.max(1, ...totals));
  const total = $derived(totals.reduce((a, b) => a + b, 0));
  const sum = (key) => counts.reduce((n, c) => n + (c[key] ?? 0), 0);
  const span = $derived(buckets.at(-1).to - buckets[0].from);
  const short = (t) => (span <= 36 * 3600000 ? clock(t).slice(0, 5) : day(t));
  // Five aligned ticks: the window's start, three quarters, and its end (`now` when the window ends at the sample clock).
  const ticks = $derived([0, 1, 2, 3, 4].map((k) => {
    const t = buckets[0].from + (k * span) / 4;
    if (k === 4 && Math.abs(t - NOW) < 60000) return 'now';
    if (k === 0 && span <= 36 * 3600000 && day(t) !== day(NOW)) return `yesterday ${short(t)}`;
    return short(t);
  }));

  let el;
  let dragged = false;
  const idx = (e) => { const r = el.getBoundingClientRect(); return Math.max(0, Math.min(buckets.length - 1, Math.floor(((e.clientX - r.left) / r.width) * buckets.length))); };
  const zoom = (a, b) => setBrush(Math.round(buckets[a].from), Math.round(buckets[b].to));
  function down(e) { if (e.button !== 0) return; dragged = false; ui.drag = [idx(e), idx(e)]; el.setPointerCapture(e.pointerId); }
  function move(e) { if (!ui.drag) return; const i = idx(e); if (i !== ui.drag[1]) { ui.drag = [ui.drag[0], i]; dragged = true; } }
  function up() {
    if (!ui.drag) return;
    const [a, b] = ui.drag;
    ui.drag = null;
    if (dragged) zoom(Math.min(a, b), Math.max(a, b));
    else if (totals[a]) zoom(a, a);
  }
  const inDrag = (k) => ui.drag && k >= Math.min(...ui.drag) && k <= Math.max(...ui.drag);
</script>

<div class="tl" data-testid={testid} data-total={total}>
  <div class="tl-head">
    <span class="h">{title}</span><span class="mono tl-total">{total.toLocaleString('en-US')}</span>
    <span class="legend mono">
      {#each series as s (s.key)}<span class="lg" data-series={s.key} data-count={sum(s.key)} title="{s.title ?? s.label}: {sum(s.key)}"><i class="sw {s.cls}" aria-hidden="true"></i>{s.label}</span>{/each}
    </span>
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bars" bind:this={el} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={() => (ui.drag = null)}>
    {#each buckets as b, k (k)}
      <button class="bk" class:indrag={inDrag(k)} tabindex={totals[k] ? 0 : -1}
        aria-label="{short(b.from)}–{short(b.to)}: {totals[k]} executions. Select to zoom both charts."
        onclick={(e) => { if (e.detail === 0 && totals[k]) zoom(k, k); }}>
        {#each series as s (s.key)}{#if counts[k][s.key]}<i class="part {s.cls}" style="height: {(counts[k][s.key] / max) * 100}%"></i>{/if}{/each}
      </button>
    {/each}
  </div>
  <div class="axis mono">{#each ticks as t, k (k)}<span>{t}</span>{/each}</div>
</div>
