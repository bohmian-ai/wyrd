<script>
  import { Handle, Position } from '@xyflow/svelte';
  import { OUTCOME } from './data.js';
  import { ui, select, current, reqState } from './store.svelte.js';

  let { id } = $props();
  const def = $derived(current().def.reqs[id]);
  const live = $derived(!!current().exec);
  const outcome = $derived(reqState(id).outcome);
  const o = $derived(OUTCOME[outcome]);
  const on = $derived(ui.sel === `req:${id}`);
</script>

<div class="gn req" class:sel={on} class:failed={live && outcome === 'failed'} data-outcome={live ? outcome : null}>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <button class="gn-main" data-node={id} aria-pressed={on} onclick={() => select(`req:${id}`)}>
    <span class="gn-row">
      {#if live}<span class="glyph {o[2]}" aria-hidden="true">{o[0]}</span><span class="sr">{o[1]}</span>{:else}<span class="glyph c-muted" aria-hidden="true">◇</span>{/if}
      <span class="gn-id" class:c-danger={live && outcome === 'failed'}>{id}</span><span class="kt push">{def.kind}</span>
    </span>
    <span class="gn-sub">{def.desc}</span>
  </button>
</div>
