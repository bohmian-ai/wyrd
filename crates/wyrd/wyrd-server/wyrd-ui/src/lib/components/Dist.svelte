<script lang="ts">
  type Segment = { label: string; value: number; color: string };
  let { segments }: { segments: Segment[] } = $props();

  const total = $derived(segments.reduce((s, x) => s + x.value, 0) || 1);
</script>

<div class="wy-dist">
  <div class="bar">
    {#each segments as s (s.label)}
      <span style={`width:${((s.value / total) * 100).toFixed(2)}%;background:${s.color}`}></span>
    {/each}
  </div>
  <div class="legend">
    {#each segments as s (s.label)}
      <span><i style={`background:${s.color}`}></i>{s.label} {s.value}</span>
    {/each}
  </div>
</div>

<style>
  .bar {
    display: flex;
    height: 30px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
  }
  .bar span + span {
    border-left: 2px solid var(--border);
  }
  .legend {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
    margin-top: 11px;
    font-family: var(--fm);
    font-size: 10px;
    color: var(--muted);
  }
  .legend i {
    display: inline-block;
    width: 11px;
    height: 11px;
    border: 2px solid var(--border);
    border-radius: 3px;
    margin-right: 5px;
    vertical-align: -2px;
  }
</style>
