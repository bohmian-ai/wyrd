<!--
  One declared dependency, drawn along the orthogonal route the layout
  computed. Corners are rounded; the line stops where the arrowhead begins and
  the arrowhead's tip sits on the target's boundary.
-->
<script lang="ts">
  import type { EdgeProps } from '@xyflow/svelte';
  import { getContext } from 'svelte';
  import type { LaidEdge } from '../verifier-layout.js';
  import { SELECTION, type GraphSelection } from './VerifierGraphNode.svelte';

  let { source, target, data: route }: EdgeProps = $props();
  const data = $derived(route as unknown as LaidEdge);
  const graph = getContext<GraphSelection>(SELECTION);
  const R = 5;
  const A = 8;
  const W = 4;

  const drawn = $derived.by(() => {
    const points = data.points.map(([x, y]) => [x, y]);
    const [x1, y1] = points.at(-2) as number[];
    const [x2, y2] = points.at(-1) as number[];
    const length = Math.hypot(x2 - x1, y2 - y1) || 1;
    const ux = (x2 - x1) / length;
    const uy = (y2 - y1) / length;
    points[points.length - 1] = [x2 - ux * A, y2 - uy * A];
    let line = `M${points[0][0]},${points[0][1]}`;
    for (let index = 1; index < points.length - 1; index += 1) {
      const [ax, ay] = points[index - 1];
      const [bx, by] = points[index];
      const [cx, cy] = points[index + 1];
      const r = Math.min(R, Math.hypot(bx - ax, by - ay) / 2, Math.hypot(cx - bx, cy - by) / 2);
      line += ` L${bx - Math.sign(bx - ax) * r},${by - Math.sign(by - ay) * r} Q${bx},${by} ${bx + Math.sign(cx - bx) * r},${by + Math.sign(cy - by) * r}`;
    }
    const tip = points.at(-1) as number[];
    return {
      line: `${line} L${tip[0]},${tip[1]}`,
      head: `M${x2},${y2} L${x2 - ux * A - uy * W},${y2 - uy * A + ux * W} L${x2 - ux * A + uy * W},${y2 - uy * A - ux * W} Z`
    };
  });
  const hot = $derived(graph.selected === source || graph.selected === target);
</script>

<g class="dep" class:hot data-edge={data.id}>
  <path class="line" d={drawn.line} />
  <path class="arrow" d={drawn.head} />
  {#if data.labelAt}
    <text x={data.labelAt.x} y={data.labelAt.y} text-anchor={data.labelAt.anchor} dominant-baseline="middle">{data.label}</text>
  {/if}
</g>

<style>
  .line {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.4;
  }
  .arrow {
    fill: var(--muted);
    stroke: none;
  }
  .hot .line {
    stroke: var(--declare);
    stroke-width: 2;
  }
  .hot .arrow {
    fill: var(--declare);
  }
  text {
    fill: var(--declare);
    font: 11px var(--font-mono);
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 4px;
    stroke-linejoin: round;
  }
</style>
