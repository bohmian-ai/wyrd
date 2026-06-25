<script lang="ts">
  import Drawer from './Drawer.svelte';
  import Button from './Button.svelte';
  import Badge from './Badge.svelte';
  import Histo from './Histo.svelte';
  import Trend from './Trend.svelte';

  type KV = { label: string; value: string; alert?: boolean };

  let {
    feature,
    dataType,
    drifted,
    psi,
    threshold,
    reference,
    current,
    driftSeries,
    stats,
    onclose
  }: {
    feature: string;
    dataType: string;
    drifted: boolean;
    psi: number;
    threshold: number;
    reference: number[];
    current: number[];
    driftSeries: number[];
    stats: KV[];
    onclose?: () => void;
  } = $props();

  // Status overrides kind: a drifted feature turns the top-bar danger.
  const accent = $derived(drifted ? 'var(--danger)' : 'var(--control-bar)');
</script>

<Drawer title={feature} {accent} chip={dataType} chipAccent="var(--server-bar)" {onclose}>
  <div class="d-sec">
    <div class="d-row">
      <Badge tone={drifted ? 'danger' : 'ok'}>{drifted ? 'drifted' : 'stable'}</Badge>
      <span>PSI <b>{psi.toFixed(2)}</b></span>
      <span class="thr">threshold {threshold.toFixed(2)}</span>
    </div>
  </div>

  <div class="d-sec">
    <div class="d-sl">distribution · reference vs current</div>
    <Histo {reference} {current} />
    <div class="d-legend">
      <span><i style="background:color-mix(in srgb,var(--control-bar) 38%,var(--surface));border-color:var(--control-bar)"></i>reference</span>
      <span><i style="background:color-mix(in srgb,var(--rune-strong) 40%,var(--surface));border-color:var(--rune-strong)"></i>current</span>
    </div>
  </div>

  <div class="d-sec">
    <div class="d-sl">drift over time · PSI</div>
    <Trend points={driftSeries} {threshold} />
  </div>

  <div class="d-sec">
    <div class="d-sl">statistics</div>
    <dl class="d-kv">
      {#each stats as s (s.label)}
        <dt>{s.label}</dt>
        <dd style={s.alert ? 'color:var(--danger);font-weight:700' : undefined}>{s.value}</dd>
      {/each}
    </dl>
  </div>

  {#snippet actions()}
    <Button variant="primary">acknowledge</Button>
    <Button variant="rune">view samples</Button>
  {/snippet}
</Drawer>

<style>
  .d-row span {
    font-family: var(--fm);
    font-size: 11px;
    color: var(--text);
  }
  .thr {
    color: var(--muted) !important;
    font-size: 10px !important;
  }
</style>
