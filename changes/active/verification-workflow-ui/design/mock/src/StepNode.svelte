<script>
  import { Handle, Position } from '@xyflow/svelte';
  import { ACTION, OUTCOME } from './data.js';
  import { ui, select, current, reqState } from './store.svelte.js';

  let { id } = $props();
  const def = $derived(current().def.steps[id]);
  const reqs = $derived(current().def.reqs);
  // Absent in definition mode: no runtime colour, status or producer state.
  const st = $derived(current().exec?.steps[id]);
  const a = $derived(st && ACTION[st.action]);
  const on = $derived(ui.sel === `step:${id}`);
  const items = $derived(st ? new Set(st.inst.map((i) => i.item).filter((v) => v != null)).size : 0);
  const iters = $derived(st ? new Set(st.inst.map((i) => i.iter).filter((v) => v != null)).size : 0);
</script>

<div class="gn" class:sel={on} class:dim={st && (st.disposition === 'discarded' || ['waiting', 'cancelled'].includes(st.action))} class:wait={st?.action.startsWith('awaiting')}>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <button class="gn-main" data-node={id} aria-pressed={on} onclick={() => select(`step:${id}`)}>
    <span class="gn-row"><span class="gn-id">{id}</span><span class="kt">{def.kind}</span></span>
    <span class="gn-row">
      {#if a}
        <span class="st {a[2]}"><span class="glyph" aria-hidden="true">{a[0]}</span>{a[1]}</span>
      {:else}
        <span class="st c-muted">needs {def.deps.join(', ')}</span>
      {/if}
    </span>
    <span class="gn-sub mono">{def.ref}</span>
    {#if def.fanout || def.repeat || def.when}
      <span class="gn-row">
        {#if a}
          {#if def.fanout}<span class="tag">× <b>{items || '–'}</b> items</span>{/if}
          {#if def.repeat}
            <span class="tag">↻ {#if st.action === 'running'}iteration <b>{iters - 1}</b>{:else}<b>{iters}</b> {iters === 1 ? 'iteration' : 'iterations'}{/if}</span>
          {/if}
          {#if def.when}<span class="tag">{st.disposition === 'none' ? 'when' : st.disposition}</span>{/if}
        {:else}
          {#if def.when}<span class="tag">when</span>{/if}
          {#if def.fanout}<span class="tag">fan-out</span>{/if}
          {#if def.repeat}<span class="tag">↻ ≤ {def.repeat.max}</span>{/if}
        {/if}
      </span>
    {/if}
  </button>
  {#each def.reqs as r (r)}
    {@const o = OUTCOME[reqState(r).outcome]}
    <button class="gn-req" class:sel={ui.sel === `req:${r}`} data-node={r} aria-pressed={ui.sel === `req:${r}`}
      onclick={(e) => { e.stopPropagation(); select(`req:${r}`); }}>
      {#if st}
        <span class="glyph {o[2]}" aria-hidden="true">{o[0]}</span><span class="gn-id small">{r}</span><span class="sr">{o[1]}</span><span class="kt push">{reqs[r].kind}</span>
      {:else}
        <span class="glyph" aria-hidden="true">◇</span><span class="gn-id small">{r}</span><span class="kt push">{reqs[r].kind}</span>
      {/if}
    </button>
  {/each}
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>
