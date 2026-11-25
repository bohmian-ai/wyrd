<script>
  import { Handle, Position } from '@xyflow/svelte';
  import { VERDICT, LIFECYCLE, clock } from './data.js';
  import { ui, select, execs } from './store.svelte.js';

  // One pinned member execution of a session judgment. Its verdict is that member's own recorded verdict.
  let { id } = $props();
  const m = $derived(execs[id]);
  const v = $derived(m.verdict ? VERDICT[m.verdict] : LIFECYCLE[m.status]);
  const on = $derived(ui.sel === `member:${id}`);
</script>

<div class="gn input" class:sel={on}>
  <button class="gn-main" data-node={id} aria-pressed={on} onclick={() => select(`member:${id}`)}>
    <span class="gn-row"><span class="gn-id">{m.run ?? id}</span><span class="badge {v[2]} push"><span aria-hidden="true">{v[0]}</span>{v[1]}</span></span>
    <span class="gn-sub">{m.def.split('@')[0]} · {clock(m.at)}</span>
  </button>
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>
