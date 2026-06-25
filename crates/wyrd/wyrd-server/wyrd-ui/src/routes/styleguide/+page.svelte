<script lang="ts">
  import ModeProvider from '$lib/components/ModeProvider.svelte';
  import Card from '$lib/components/Card.svelte';
  import Button from '$lib/components/Button.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import Table from '$lib/components/Table.svelte';
  import KpiTile from '$lib/components/KpiTile.svelte';
  import Spark from '$lib/components/Spark.svelte';
  import Bars from '$lib/components/Bars.svelte';
  import Lines from '$lib/components/Lines.svelte';
  import Histo from '$lib/components/Histo.svelte';
  import Trend from '$lib/components/Trend.svelte';
  import Heatmap from '$lib/components/Heatmap.svelte';
  import Dist from '$lib/components/Dist.svelte';
  import Tree from '$lib/components/Tree.svelte';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import CodeBlock from '$lib/components/CodeBlock.svelte';
  import TraceTable from '$lib/components/TraceTable.svelte';
  import Waterfall from '$lib/components/Waterfall.svelte';
  import SpanPanel from '$lib/components/SpanPanel.svelte';
  import EvalPanel from '$lib/components/EvalPanel.svelte';
  import DriftPanel from '$lib/components/DriftPanel.svelte';
  import Shell from '$lib/components/Shell.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Topbar from '$lib/components/Topbar.svelte';
  import Hero from '$lib/components/Hero.svelte';
  import { registry } from '$lib/registry';

  const registered = Object.keys(registry);

  const traceRows = [
    { id: 'tr_9f21', op: 'agent.run', kind: 'agent' as const, spans: 14, durationMs: 4210, tokens: 12400, cost: 0.18, score: 0.92, scoreTone: 'hi' as const, status: 'ok' as const },
    { id: 'tr_8c04', op: 'llm.synthesize', kind: 'llm' as const, spans: 6, durationMs: 1840, tokens: 4100, cost: 0.06, score: 0.71, scoreTone: 'mid' as const, status: 'warn' as const },
    { id: 'tr_71be', op: 'tool.search', kind: 'tool' as const, spans: 3, durationMs: 320, status: 'err' as const }
  ];

  const spans = [
    { depth: 0, name: 'agent.run', kind: 'agent' as const, start: 0, duration: 4210, status: 'ok' as const },
    { depth: 1, name: 'retrieval.fetch', kind: 'retrieval' as const, start: 120, duration: 880, status: 'ok' as const },
    { depth: 1, name: 'llm.synthesize', kind: 'llm' as const, start: 1080, duration: 1840, status: 'warn' as const },
    { depth: 2, name: 'tool.search', kind: 'tool' as const, start: 1200, duration: 320, status: 'err' as const }
  ];

  const navGroups = [
    {
      label: 'registry',
      items: [
        { label: 'cards', href: '#', active: true, kind: 'client' as const },
        { label: 'evals', href: '#', kind: 'server' as const },
        { label: 'policies', href: '#', kind: 'control' as const }
      ]
    },
    { label: 'observe', items: [{ label: 'traces', href: '#' }, { label: 'drift', href: '#' }] }
  ];
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

      <Card>
        {#snippet head()}<span>Bars</span><span>analytics</span>{/snippet}
        <Bars
          data={[
            { label: 'agent', value: 42 },
            { label: 'llm', value: 31, color: 'var(--rune-strong)' },
            { label: 'tool', value: 18, color: 'var(--server-bar)' },
            { label: 'rag', value: 9, color: 'var(--control-bar)' }
          ]}
        />
      </Card>

      <Card>
        {#snippet head()}<span>Lines</span><span>percentiles</span>{/snippet}
        <Lines
          labels={['00', '06', '12', '18', '24']}
          series={[
            { points: [12, 18, 14, 22, 19], color: 'var(--control-bar)' },
            { points: [28, 33, 30, 41, 36], color: 'var(--rune-strong)' },
            { points: [52, 60, 48, 71, 64], color: 'var(--danger)' }
          ]}
        />
      </Card>

      <Card>
        {#snippet head()}<span>Spark · Heatmap</span><span>micro</span>{/snippet}
        <div class="row">
          <Spark points={[18, 14, 16, 9, 11, 5, 7]} sentiment="ok" />
          <Spark points={[5, 8, 7, 12, 10, 16, 19]} sentiment="danger" />
        </div>
        <div style="margin-top:12px">
          <Heatmap values={[2, 5, 8, 3, 9, 12, 6, 14, 4, 10, 7, 13, 1, 8, 11, 5, 9, 3, 12, 6, 14, 2, 7, 10]} />
        </div>
      </Card>

      <Card>
        {#snippet head()}<span>Histo · Trend</span><span>drift</span>{/snippet}
        <Histo reference={[4, 10, 22, 30, 20, 10, 4]} current={[2, 5, 12, 20, 28, 22, 12]} />
        <div style="margin-top:12px">
          <Trend points={[0.08, 0.1, 0.12, 0.15, 0.14, 0.19, 0.22, 0.24]} threshold={0.2} />
        </div>
      </Card>

      <Card>
        {#snippet head()}<span>Dist</span><span>planes</span>{/snippet}
        <Dist
          segments={[
            { label: 'client', value: 48, color: 'var(--client-bar)' },
            { label: 'server', value: 34, color: 'var(--server-bar)' },
            { label: 'control', value: 18, color: 'var(--control-bar)' }
          ]}
        />
      </Card>

      <Card>
        {#snippet head()}<span>Tree</span><span>card hierarchy</span>{/snippet}
        <Tree
          nodes={[
            {
              label: 'payments-svc',
              meta: 'service',
              open: true,
              children: [
                { label: 'churn-v3', meta: 'model', kind: 'server' },
                { label: 'fathom-wf', meta: 'agent', kind: 'client' },
                { label: 'pii-gate', meta: 'policy', kind: 'control' }
              ]
            }
          ]}
        />
      </Card>

      <Card>
        {#snippet head()}<span>Dropdown</span><span>control</span>{/snippet}
        <Dropdown
          label="tenant: acme"
          value="acme"
          options={[
            { label: 'acme', kind: 'client' },
            { label: 'globex', kind: 'server' },
            { label: 'initech', kind: 'control' }
          ]}
        />
      </Card>

      <Card>
        {#snippet head()}<span>CodeBlock</span><span>copy</span>{/snippet}
        <CodeBlock code={'wyrd apply --card payments-svc \\\n  --scope bifrost_record:write'} />
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>TraceTable</span><span>observe</span>{/snippet}
        <TraceTable rows={traceRows} selectedId="tr_8c04" />
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>Waterfall</span><span>spans</span>{/snippet}
        <Waterfall {spans} />
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>Drilldown panels</span><span>raised</span>{/snippet}
        <div class="panels">
          <SpanPanel
            name="llm.synthesize"
            kind="llm"
            summary="Synthesis over 4 retrieved policy docs."
            attributes={[
              { label: 'span_id', value: 'span_7c2a4f' },
              { label: 'trace_id', value: 'tr_9f21', link: true }
            ]}
            tokens={{ input: 1400, output: 1000, cache: 200 }}
            events={[
              { label: 'first token', at: '+0.4s' },
              { label: 'complete', at: '+1.8s' }
            ]}
            outputPreview="Based on the retrieved policy docs, the request is within limits…"
          />
          <EvalPanel
            recordId="rec_4a91c7"
            agent="fathom-wf"
            score={0.92}
            pass={true}
            judge="llm-judge · gpt-class"
            metrics={[
              { label: 'relevance', value: 0.95 },
              { label: 'groundedness', value: 0.79 }
            ]}
            threshold={0.8}
            rationale="Answer is well grounded; minor unsupported claim on pricing."
            linked={[{ label: 'trace', value: 'tr_9f21', link: true }]}
          />
          <DriftPanel
            feature="request_amount"
            dataType="NUMERIC"
            drifted={true}
            psi={0.27}
            threshold={0.2}
            reference={[4, 10, 22, 30, 20, 10, 4]}
            current={[2, 5, 12, 20, 28, 22, 12]}
            driftSeries={[0.08, 0.1, 0.12, 0.15, 0.14, 0.19, 0.22, 0.24]}
            stats={[
              { label: 'ref mean', value: '142.3' },
              { label: 'cur mean', value: '188.7', alert: true }
            ]}
          />
        </div>
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>Shell · Sidebar · Topbar</span><span>app frame</span>{/snippet}
        <Shell>
          {#snippet sidebar()}<Sidebar brand="wyrd" groups={navGroups} />{/snippet}
          {#snippet topbar()}
            <Topbar
              crumbs={['registry', 'cards', 'payments-svc']}
              search="⌘K  search cards…"
              env={{ label: 'prod', status: 'ok' }}
            />
          {/snippet}
          <p style="margin:0;font-family:var(--fm);font-size:12px;color:var(--muted)">
            main content region — the workbench renders here.
          </p>
        </Shell>
      </Card>

      <Card variant="wide">
        {#snippet head()}<span>Hero</span><span>loud</span>{/snippet}
        <Hero
          title="The Wyrd Registry"
          ornament="✳ wyrd"
          tagline="Card-bound identity for the agentic stack. One credential. Two planes."
          stats={[
            { value: '18', label: 'Card kinds' },
            { value: '2', label: 'Planes' },
            { value: '1', label: 'Credential' }
          ]}
          ctas={[
            { label: 'wyrd apply →' },
            { label: 'view docs', variant: 'rune' }
          ]}
        />
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
  .panels {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .panels > :global(.wy-drawer) {
    flex: 0 1 340px;
    min-width: 0;
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
