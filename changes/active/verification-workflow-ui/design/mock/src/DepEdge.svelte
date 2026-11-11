<script>
  import { untrack } from 'svelte';
  import { ui, current, selection } from './store.svelte.js';

  // One declared dependency, drawn along the orthogonal route the layout computed. Corners are rounded; the line stops
  // where the arrowhead begins, and the arrowhead's tip sits on the target's boundary.
  let { source, target, data } = $props();
  const R = 5, A = 8, W = 4;

  // Routes are laid out once per definition and never change, so reading them once is exact.
  const pts = untrack(() => data.points);
  const [x1, y1] = pts.at(-2), [x2, y2] = pts.at(-1);
  const len = Math.hypot(x2 - x1, y2 - y1) || 1;
  const ux = (x2 - x1) / len, uy = (y2 - y1) / len;

  function line() {
    const p = pts.map((q) => [...q]);
    p[p.length - 1] = [x2 - ux * A, y2 - uy * A];
    let d = `M${p[0][0]},${p[0][1]}`;
    for (let i = 1; i < p.length - 1; i++) {
      const [ax, ay] = p[i - 1], [bx, by] = p[i], [cx, cy] = p[i + 1];
      const r = Math.min(R, Math.hypot(bx - ax, by - ay) / 2, Math.hypot(cx - bx, cy - by) / 2);
      d += ` L${bx - Math.sign(bx - ax) * r},${by - Math.sign(by - ay) * r} Q${bx},${by} ${bx + Math.sign(cx - bx) * r},${by + Math.sign(cy - by) * r}`;
    }
    return `${d} L${p.at(-1)[0]},${p.at(-1)[1]}`;
  }
  const d = line();
  const head = `M${x2},${y2} L${x2 - ux * A - uy * W},${y2 - uy * A + ux * W} L${x2 - ux * A + uy * W},${y2 - uy * A - ux * W} Z`;

  const def = current().def;
  const exec = $derived(current().exec);
  const sel = $derived(selection().id);
  const hot = $derived(sel === source || sel === target);
  // Red only where a failed requirement read this exact producer. A session's members feed its judgment through one
  // bus; their own verdicts never colour it, so a passing member never looks like the cause of a failed judgment.
  const bad = $derived(!!exec && !data.bus && exec.reqs[target]?.outcome === 'failed');
  const dim = $derived.by(() => {
    if (!exec) return false;
    const st = exec.steps[source];
    if (st) return ['waiting', 'cancelled', 'skipped'].includes(st.action) || st.disposition === 'discarded';
    return !!exec.inputs?.[source]?.awaited;
  });
  const cls = $derived([hot && 'hot', bad && 'bad', dim && 'dim'].filter(Boolean).join(' '));
</script>

<g class="dep-edge {cls}" data-edge="{source}->{target}" data-bus={data.bus}>
  <path class="svelte-flow__edge-path dep {cls}" {d} />
  <path class="arrow {cls}" d={head} />
  {#if data.labelAt}
    <text class="elabel" x={data.labelAt.x} y={data.labelAt.y} text-anchor={data.labelAt.anchor} dominant-baseline="middle">{data.label}</text>
  {/if}
</g>
