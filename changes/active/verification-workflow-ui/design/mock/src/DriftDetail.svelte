<script>
  import { day } from './data.js';
  import { execs } from './store.svelte.js';

  // Recorded statistics only: scores, thresholds and per-feature verdicts are what each window's execution recorded.
  // No baseline distribution is drawn and nothing is re-judged here.
  let { x, req } = $props();
  const res = $derived(x.reqs[req]);
  const features = $derived([...res.drift.features].sort((a, b) => (b.verdict === 'drift') - (a.verdict === 'drift') || b.score - a.score));
  const windows = $derived(Object.values(execs).filter((e) => e.def === x.def).sort((a, b) => a.at - b.at));
  const W = 168, H = 34, TOP = 0.4;
  const xs = (k) => 4 + (k * (W - 8)) / Math.max(1, windows.length - 1);
  const ys = (v) => H - 3 - (Math.min(v, TOP) / TOP) * (H - 6);
  const history = (name) => windows.map((e) => e.reqs[req].drift.features.find((f) => f.name === name).score);
</script>

<div class="drift" data-testid="drift">
  <dl class="dkv">
    <dt>Window</dt><dd>{day(x.window[0])} 00:00 → {day(x.window[1])} 00:00 UTC · {res.drift.samples.toLocaleString('en-US')} samples</dd>
  </dl>
  <div class="outs-wrap">
    <table class="outs" data-testid="drift-features">
      <thead><tr><th>Feature</th><th>Score per window · dashed = threshold</th><th>Score</th><th>Threshold</th><th>Recorded</th></tr></thead>
      <tbody>
        {#each features as f (f.name)}
          {@const h = history(f.name)}
          {@const cur = windows.findIndex((e) => e.id === x.id)}
          <tr data-feature={f.name}>
            <td><span>{f.name}</span></td>
            <td><span>
              <svg class="spark" width={W} height={H} viewBox="0 0 {W} {H}" role="img" aria-label="{f.name} PSI over {windows.length} windows, {day(windows[0].window[1])} to {day(windows.at(-1).window[1])}">
                <line class="thr" x1="0" x2={W} y1={ys(f.threshold)} y2={ys(f.threshold)} />
                <polyline class="line" points={h.map((v, k) => `${xs(k)},${ys(v)}`).join(' ')} />
                {#each h as v, k (k)}<circle class:over={v > f.threshold} cx={xs(k)} cy={ys(v)} r={k === cur ? 3.5 : 1.8} class:cur={k === cur} />{/each}
              </svg>
            </span></td>
            <td><span class={f.verdict === 'drift' ? 'c-danger' : ''}>{f.score.toFixed(2)}</span></td>
            <td><span>{f.threshold.toFixed(2)}</span></td>
            <td><span class={f.verdict === 'drift' ? 'c-danger' : 'c-ok'}><span aria-hidden="true">{f.verdict === 'drift' ? '✕' : '✓'}</span> {f.verdict}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="note small">History spans the {windows.length} recorded windows of {x.def}; the larger point is this window.</p>
</div>
