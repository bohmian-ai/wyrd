<script>
  import { Handle, Position } from '@xyflow/svelte';
  import { ui, select, current } from './store.svelte.js';

  let { id } = $props();
  const def = $derived(current().def.inputs[id]);
  const awaited = $derived(!!current().exec?.inputs?.[id]?.awaited);
  const on = $derived(ui.sel === `input:${id}`);
</script>

<div class="gn input" class:sel={on} class:wait={awaited}>
  <button class="gn-main" data-node={id} aria-pressed={on} onclick={() => select(`input:${id}`)}>
    <span class="gn-row"><span class="gn-id">{id}</span><span class="kt push">{def.kind}</span></span>
    <span class="gn-sub" class:c-warn={awaited}>{awaited ? 'not provided yet' : def.sub}</span>
  </button>
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>
