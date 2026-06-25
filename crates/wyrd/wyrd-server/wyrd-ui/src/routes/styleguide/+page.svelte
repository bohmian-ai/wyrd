<script lang="ts">
  import ModeProvider from '$lib/components/ModeProvider.svelte';
  import Card from '$lib/components/Card.svelte';
  import Button from '$lib/components/Button.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import Table from '$lib/components/Table.svelte';
  import KpiTile from '$lib/components/KpiTile.svelte';
  import { registry } from '$lib/registry';

  const registered = Object.keys(registry);
</script>

{#snippet spark()}
  <svg viewBox="0 0 90 24" width="90" height="24" fill="none">
    <polyline
      points="0,18 15,14 30,16 45,9 60,11 75,5 90,7"
      stroke="var(--ok)"
      stroke-width="2"
      stroke-linejoin="round"
      stroke-linecap="round"
    />
  </svg>
{/snippet}

{#snippet catalog()}
  <div class="pad">
    <div class="sgh">
      <span class="wm">WYRD</span> design system · registered: {registered.join(', ')}
    </div>

    <div class="grid">
      <Card>
        {#snippet head()}<span>Buttons</span><span>quiet</span>{/snippet}
        <div class="row">
          <Button variant="primary">Primary</Button>
          <Button variant="rune">Rune</Button>
          <Button variant="ghost">Ghost</Button>
        </div>
      </Card>

      <Card>
        {#snippet head()}<span>Badges</span><span>status</span>{/snippet}
        <div class="row">
          <Badge>neutral</Badge>
          <Badge tone="ok">healthy</Badge>
          <Badge tone="warn">drift</Badge>
          <Badge tone="danger">failed</Badge>
        </div>
      </Card>

      <Card variant="raised">
        {#snippet head()}<span>Card · raised</span><span>6px</span>{/snippet}
        <p style="margin:0;font-size:13px;line-height:1.5">
          Raised altitude — used for drilldown panels. Same geometry, deeper shadow.
        </p>
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>KPI tiles</span><span>analytics</span>{/snippet}
        <div class="kpis">
          <KpiTile label="runs" value="1,284" delta="+8%" trend="up" {spark} />
          <KpiTile label="p95 latency" value="842ms" delta="-3%" trend="down" {spark} />
          <KpiTile label="error rate" value="1.2%" delta="+0.4%" trend="up" {spark} />
          <KpiTile label="spend" value="$42.10" delta="+12%" trend="up" {spark} />
        </div>
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>Table</span><span>quiet</span>{/snippet}
        <Table>
          <table>
            <thead>
              <tr><th>trace</th><th>root op</th><th>spans</th><th>status</th></tr>
            </thead>
            <tbody>
              <tr><td>tr_9f21</td><td>agent.run</td><td>14</td><td><Badge tone="ok">ok</Badge></td></tr>
              <tr class="sel"><td>tr_8c04</td><td>llm.synthesize</td><td>6</td><td><Badge tone="warn">slow</Badge></td></tr>
              <tr><td>tr_71be</td><td>tool.search</td><td>3</td><td><Badge tone="danger">err</Badge></td></tr>
            </tbody>
          </table>
        </Table>
      </Card>
    </div>
  </div>
{/snippet}

<div class="cols">
  <div class="col">
    <div class="tag">LIGHT</div>
    <ModeProvider mode="light">{@render catalog()}</ModeProvider>
  </div>
  <div class="col">
    <div class="tag">DARK</div>
    <ModeProvider mode="dark">{@render catalog()}</ModeProvider>
  </div>
</div>

<style>
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: #888;
  }
  .col {
    background: #444;
  }
  .tag {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 2px;
    color: #fff;
    padding: 8px 18px;
  }
  .pad {
    padding: 18px;
  }
  .sgh {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted);
    margin-bottom: 16px;
  }
  .sgh .wm {
    font-family: var(--font-display);
    font-size: 14px;
    color: var(--text);
    margin-right: 6px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .kpis {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    border: 2px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
  }
  .kpis :global(.wy-kpi) {
    border-right: 2px solid var(--border);
  }
  .kpis :global(.wy-kpi:last-child) {
    border-right: 0;
  }
</style>
