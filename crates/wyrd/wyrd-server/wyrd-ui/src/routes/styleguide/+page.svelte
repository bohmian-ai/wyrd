<script lang="ts">
  import ModeProvider from '$lib/components/ModeProvider.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import Button from '$lib/components/Button.svelte';
  import Chip from '$lib/components/Chip.svelte';
  import CodeBlock from '$lib/components/CodeBlock.svelte';
  import Disclosure from '$lib/components/Disclosure.svelte';
  import KpiTile from '$lib/components/KpiTile.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import Select from '$lib/components/Select.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import Bars from '$lib/components/charts/Bars.svelte';
  import ChartPanel from '$lib/components/charts/ChartPanel.svelte';
  import Line from '$lib/components/charts/Line.svelte';
  import Spark from '$lib/components/charts/Spark.svelte';
  import Shell from '$lib/components/Shell.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Topbar from '$lib/components/Topbar.svelte';
  import { registry } from '$lib/registry';

  // The style guide is the component foundation's rendered evidence: every catalog entry
  // in both modes, side by side, with the states and the narrow container that the
  // accepted mocks require. It is a harness, not a product route.
  const registered = Object.keys(registry);

  const latency = [
    { label: 'p50', points: [120, 118, 131, 126, 140, 133, 129] },
    { label: 'p95', points: [310, 402, 388, 361, 470, 441, 402] },
    { label: 'p99', points: [520, 610, 588, 640, 812, 733, 690] }
  ];
  const rangeOptions = [
    { value: '1h', label: 'Last 1 hour' },
    { value: '24h', label: 'Last 24 hours' },
    { value: '7d', label: 'Last 7 days' }
  ];
  const kindOptions = [
    { value: 'Service', label: 'Service' },
    { value: 'Model', label: 'Model' },
    { value: 'Prompt', label: 'Prompt' }
  ];
  const evidence = [
    { kind: 'unit-tests', subject: 'acme/ranking', verdict: 'ok' as const, label: 'passed' },
    { kind: 'pii-review', subject: 'acme/checkout', verdict: 'danger' as const, label: 'failed' },
    { kind: 'load-test', subject: 'acme/ledger', verdict: 'running' as const, label: 'running' }
  ];
  const navGroups = [
    {
      label: 'workbench',
      items: [
        { label: 'cards', href: '#', active: true },
        { label: 'observe', href: '#' },
        { label: 'changes', href: '#' }
      ]
    }
  ];
</script>

{#snippet trend()}
  <Spark points={[18, 14, 16, 9, 11, 5, 7]} />
{/snippet}

{#snippet catalog()}
  <div class="pad">
    <div class="sgh">
      <span class="wm">WYRD</span> component foundation · catalog: {registered.join(', ')}
    </div>

    <div class="grid">
      <Panel title="Badge" variant="quiet">
        <div class="row">
          <Badge>draft</Badge>
          <Badge tone="ok">passed</Badge>
          <Badge tone="warn">stale</Badge>
          <Badge tone="danger">failed</Badge>
          <Badge tone="running">running</Badge>
        </div>
      </Panel>

      <Panel title="Button">
        <div class="row">
          <Button variant="primary">Run now</Button>
          <Button variant="secondary">Open fix PR</Button>
          <Button variant="ghost">Cancel</Button>
          <Button href="#chrome">View Card →</Button>
        </div>
      </Panel>

      <Panel title="Chip · applied filters">
        <div class="row">
          <Chip label="kind" value="Service" removeHref="?space=prod" />
          <Chip label="space" value="prod" removeHref="?kind=Service" />
          <Chip label="owner" value="m.linden" />
        </div>
      </Panel>

      <Panel title="Select · filter and time controls">
        <form class="row" method="GET">
          <Select label="Kind" name="kind" options={kindOptions} value="Service" />
          <Select label="Range" name="range" options={rangeOptions} value="24h" />
        </form>
      </Panel>

      <Panel title="Metric values" variant="quiet">
        <div class="kpis">
          <KpiTile label="Cards" value="42" delta="+3 this week" trend="up" />
          <KpiTile label="Open changes" value="7" delta="-2 vs last week" trend="down" />
          <KpiTile label="p95 latency" value="402" delta="ms" trend="flat" spark={trend} />
          <KpiTile label="Spend" value="$42.10" delta="+$4.02 vs last week" trend="up" />
        </div>
      </Panel>

      <Panel title="Table · evidence">
        <Table label="Evidence for change_01">
          <table>
            <thead>
              <tr><th>kind</th><th>subject</th><th>verdict</th></tr>
            </thead>
            <tbody>
              {#each evidence as e, i (e.kind)}
                <tr class={i === 0 ? 'sel' : ''}>
                  <td>{e.kind}</td>
                  <td><a href="#chrome">{e.subject}</a></td>
                  <td><Badge tone={e.verdict}>{e.label}</Badge></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </Table>
      </Panel>

      <Panel title="Disclosure">
        <Disclosure summary="Raw definition" meta="wyrd/v1">
          <CodeBlock code={'{\n  "apiVersion": "wyrd/v1",\n  "kind": "Prompt"\n}'} />
        </Disclosure>
      </Panel>

      <Panel title="Async and authorization states">
        <div class="stack">
          <StateBlock state="loading" title="Loading Cards" />
          <StateBlock state="empty" title="No Cards match these filters" detail="Remove a filter to widen the search." actionLabel="Clear filters" actionHref="?" />
          <StateBlock state="partial" title="Partial results" detail="200 of 412 records loaded." />
          <StateBlock state="unauthorized" title="Not authorized to read this Card" detail="Ask an owner for the cards:read scope." code="WYRD-AUTHZ-FORBIDDEN" />
          <StateBlock state="error" title="Cards could not be loaded" code="WYRD-CHANGE-502" />
          <StateBlock state="absent" title="No figures attached" detail="This Data Card version declares no figures." />
        </div>
      </Panel>

      <div class="wide">
        <ChartPanel
          title="Request latency"
          measure="Latency percentiles"
          unit="ms"
          latestValue={402}
          source="vala · checkout-api"
          freshness={{ label: '2m ago', at: '2026-09-04T17:18:00Z' }}
          from={{ label: 'Sep 3 17:20', at: '2026-09-03T17:20:00Z' }}
          to={{ label: 'Sep 4 17:20', at: '2026-09-04T17:20:00Z' }}
          link={{ label: 'Open in Observe', href: '#chrome' }}
        >
          <Line series={latency} labels={[
            { label: '-24h', at: '2026-09-03T17:20:00Z' },
            { label: '', at: '2026-09-03T23:20:00Z' },
            { label: '-12h', at: '2026-09-04T05:20:00Z' },
            { label: '', at: '2026-09-04T08:20:00Z' },
            { label: '-6h', at: '2026-09-04T11:20:00Z' },
            { label: '', at: '2026-09-04T14:20:00Z' },
            { label: 'now', at: '2026-09-04T17:20:00Z' }
          ]} threshold={{ value: 750, label: 'SLO 750ms' }} />
        </ChartPanel>
      </div>

      <ChartPanel
        title="Evidence by kind"
        measure="Evidence records"
        unit="records"
        latestValue={31}
        source="wyrd · change_01"
        from={{ label: 'Sep 3', at: '2026-09-03' }}
        to={{ label: 'Sep 4', at: '2026-09-04' }}
      >
        <Bars data={[{ label: 'unit', value: 14 }, { label: 'pii', value: 6 }, { label: 'load', value: 11 }]} unit="records" />
      </ChartPanel>

      <ChartPanel
        title="Drift score"
        measure="PSI"
        source="vala · feature"
        state="unauthorized"
        detail="This principal cannot read drift results for txns-2026q3."
        code="WYRD-AUTHZ-FORBIDDEN"
      />
    </div>

    <div class="sgh narrow-h">Narrow container — the same components at 340px</div>
    <div class="narrow">
      <Panel title="Latency">
        <div class="row">
          <Chip label="kind" value="Service" removeHref="?" />
          <Badge tone="warn">stale</Badge>
        </div>
        <div class="stack">
          <Table label="Evidence, narrow">
            <table>
              <thead><tr><th>kind</th><th>subject</th><th>verdict</th></tr></thead>
              <tbody>
                {#each evidence as e (e.kind)}
                  <tr><td>{e.kind}</td><td>{e.subject}</td><td><Badge tone={e.verdict}>{e.label}</Badge></td></tr>
                {/each}
              </tbody>
            </table>
          </Table>
        </div>
      </Panel>
    </div>

    <div class="sgh narrow-h" id="chrome">Trusted application chrome — implemented, deliberately not in the catalog</div>
    <Shell>
      {#snippet sidebar()}<Sidebar brand="bohmian" groups={navGroups} />{/snippet}
      {#snippet topbar()}<Topbar crumbs={['acme', 'cards']} search="Search Cards" env={{ label: 'prod', status: 'ok' }} />{/snippet}
      <div class="row"><Badge tone="ok">chrome resolves tenant identity, not an authored view</Badge></div>
    </Shell>
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
    min-width: 0;
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
  .narrow-h {
    margin-top: 22px;
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
    align-items: start;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 10px;
  }
  .narrow {
    max-width: 340px;
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

  /* Harness is laptop/monitor-first; below these widths it stacks rather than crushes. */
  @media (max-width: 1024px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .kpis {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
