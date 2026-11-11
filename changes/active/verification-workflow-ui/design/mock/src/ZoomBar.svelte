<script>
  import { useSvelteFlow } from '@xyflow/svelte';

  let { zoom, fit, min, max, host, definition } = $props();
  const flow = useSvelteFlow();

  // Zooms around the centre of the canvas. zoomIn()/zoomOut() did not move the viewport in this setup; setViewport does.
  function zoomTo(k) {
    const { x, y, zoom: z } = flow.getViewport();
    const next = Math.min(max, Math.max(min, k));
    const cx = host.clientWidth / 2, cy = host.clientHeight / 2;
    flow.setViewport({ x: cx - ((cx - x) / z) * next, y: cy - ((cy - y) / z) * next, zoom: next });
  }
</script>

<div class="panel-bar">
  <span class="h">Graph</span>
  <span class="small muted">{definition ? 'Declared dependencies. No execution state is shown.' : 'Declared dependencies with recorded state. Select a step or requirement to inspect it.'}</span>
  <span class="grow"></span>
  <div class="zoom" role="group" aria-label="Zoom">
    <span class="zoom-val" data-testid="zoom">{Math.round(zoom * 100)}%</span>
    <button class="btn" aria-label="Zoom out" onclick={() => zoomTo(zoom / 1.2)}>−</button>
    <button class="btn" aria-label="Zoom in" onclick={() => zoomTo(zoom * 1.2)}>+</button>
    <button class="btn" data-testid="fit" onclick={() => flow.setViewport(fit())}>Fit</button>
    <button class="btn" onclick={() => zoomTo(1)}>100%</button>
  </div>
</div>
