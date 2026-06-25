<script lang="ts">
  import Drawer from './Drawer.svelte';
  import Button from './Button.svelte';
  import Badge from './Badge.svelte';
  import CodeBlock from './CodeBlock.svelte';

  type Metric = { label: string; value: number };
  type KV = { label: string; value: string; link?: boolean };

  let {
    recordId,
    agent,
    score,
    pass,
    judge,
    metrics,
    threshold,
    rationale,
    linked,
    onclose
  }: {
    recordId: string;
    agent: string;
    score: number;
    pass: boolean;
    judge?: string;
    metrics: Metric[];
    threshold: number;
    rationale?: string;
    linked?: KV[];
    onclose?: () => void;
  } = $props();

  const accent = $derived(pass ? 'var(--rune-strong)' : 'var(--danger)');
</script>

<Drawer title={recordId} {accent} chip={agent} {onclose}>
  <div class="d-sec">
    <div class="d-big">
      <span class="num">{score.toFixed(2)}</span>
      <div>
        <div><Badge tone={pass ? 'ok' : 'danger'}>{pass ? 'pass' : 'fail'}</Badge></div>
        {#if judge}<div class="sub">{judge}</div>{/if}
      </div>
    </div>
  </div>

  <div class="d-sec">
    <div class="d-sl">metric scores · threshold {threshold.toFixed(2)}</div>
    {#each metrics as m (m.label)}
      {@const ok = m.value >= threshold}
      <div class="d-metric">
        <span>{m.label}</span>
        <span class="bar"><i style={`width:${(m.value * 100).toFixed(0)}%;background:${ok ? 'var(--ok)' : 'var(--danger)'}`}></i></span>
        <span class="v" style={ok ? undefined : 'color:var(--danger)'}>{m.value.toFixed(2)}</span>
      </div>
    {/each}
  </div>

  {#if rationale}
    <div class="d-sec">
      <div class="d-sl">judge rationale</div>
      <CodeBlock code={rationale} copy={false} />
    </div>
  {/if}

  {#if linked && linked.length}
    <div class="d-sec">
      <div class="d-sl">linked trace</div>
      <dl class="d-kv">
        {#each linked as l (l.label)}
          <dt>{l.label}</dt>
          <dd>{#if l.link}<span class="lnk">{l.value}</span>{:else}{l.value}{/if}</dd>
        {/each}
      </dl>
    </div>
  {/if}

  {#snippet actions()}
    <Button variant="rune">open trace</Button>
    <Button variant="ghost">re-run eval</Button>
  {/snippet}
</Drawer>

<style>
  .sub {
    margin-top: 5px;
    font-family: var(--fm);
    font-size: 10px;
    color: var(--muted);
  }
</style>
