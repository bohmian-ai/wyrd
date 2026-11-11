<script>
  import { LIFECYCLE, VERDICT } from './data.js';

  // Status and nullable verdict of one execution, always as two separate facts.
  let { x, part } = $props();
  const s = $derived(LIFECYCLE[x.status]);
  const v = $derived(x.verdict && VERDICT[x.verdict]);
</script>

{#if part !== 'verdict'}<span class="st {s[2]}" data-status={x.status}><span aria-hidden="true">{s[0]}</span>{s[1]}</span>{/if}
{#if part !== 'status'}
  {#if v}<span class="badge {v[2]}" data-verdict={x.verdict}><span aria-hidden="true">{v[0]}</span>{v[1]}</span>
  {:else}<span class="st c-muted" data-verdict="none">— none</span>{/if}
{/if}
