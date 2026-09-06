<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import { scopeQuery } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/evaluations\/[^/]+$/, ''));
  const backHref = $derived(`${base}/observe/evaluations${scopeQuery(data.scope)}`);
  function taskHref(name: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('task', name);
    return `?${query}`;
  }
  const scorePct = (task: { scoreValue: number | null; threshold: number | null }) => {
    if (task.scoreValue === null || !task.threshold) return 0;
    const max = Math.max(task.scoreValue, task.threshold) * 1.15 || 1;
    return Math.min(100, (task.scoreValue / max) * 100);
  };
  const tickPct = (task: { scoreValue: number | null; threshold: number | null }) => {
    if (task.scoreValue === null || !task.threshold) return 0;
    const max = Math.max(task.scoreValue, task.threshold) * 1.15 || 1;
    return Math.min(100, (task.threshold / max) * 100);
  };
</script>

<svelte:head><title>{data.view?.recordId ?? 'Evaluation'} · Observe · Wyrd</title></svelte:head>
<div class="observe">
  {#if data.problem}
    <h1>Evaluation event</h1>
    <ObserveNav base={`${base}/observe`} current="Evaluations" scope={data.scope} />
    <StateBlock
      state={data.problem.status === 403 ? 'unauthorized' : data.problem.status === 404 ? 'absent' : 'error'}
      title={data.problem.title}
      code={data.problem.code}
      actionLabel="Back to evaluations"
      actionHref={backHref}
    />
  {:else if data.view}
    <!-- The event page: identity and workflow results. It stays rendered behind
         the drawer scrim so Workflow and Task are views of the same event. -->
    <h1>{data.view.eval}</h1>
    <p class="muted">{data.view.recordId} · one event, its workflow and every task result.</p>
    <ObserveNav base={`${base}/observe`} current="Evaluations" scope={data.scope} />
    <div class="stack">
      <a class="mono" href={backHref}>← Back to evaluations — the filtered inventory is restored</a>
      <div class="columns">
        <Panel title="Event">
          <div class="kv">
            <span class="k">eval card</span>
            {#if data.view.evalCardHref}
              <a class="mono" href={`${base}/cards/${data.view.evalCardHref}`}>{data.view.eval}</a>
            {:else}
              <span class="mono">offline — no registered Eval card</span>
            {/if}
            <span class="k">subject</span>
            {#if data.view.subjectHref}
              <a class="mono" href={`${base}/cards/${data.view.subjectHref}`}>{data.view.subject}</a>
            {:else}
              <span class="mono">{data.view.subject}</span>
            {/if}
            <span class="k">run</span><span class="mono">{data.view.runId}</span>
            {#if data.view.scenario}
              <span class="k">scenario</span><span class="mono">{data.view.scenario.id} · {data.view.scenario.collection}</span>
              <span class="k">query</span><span>{data.view.scenario.initialQuery}</span>
            {/if}
          </div>
          <p class="passed"><strong>{data.view.passed}/{data.view.total}</strong> <span class="mono muted">TASKS PASSED</span></p>
        </Panel>
        <Panel title="Workflow results">
          <p class="mono"><strong>{data.view.workflow.version}</strong></p>
          <p class="muted">started {data.view.workflow.started} · {data.view.duration}</p>
          <p><Badge tone={data.view.status.tone}>{data.view.status.label}</Badge></p>
        </Panel>
      </div>
    </div>

    <!-- The workflow drawer: stages, task list and the dominant selected task. -->
    <div class="drawer-scrim" aria-hidden="true"></div>
    <section class="drawer" aria-label={`Workflow ${data.view.workflow.name}`}>
      <header class="row between drawer-head">
        <div>
          <p class="mono muted head-k">Workflow</p>
          <h2>{data.view.workflow.name} <Badge tone={data.view.status.tone}>{data.view.status.label}</Badge></h2>
          <p class="mono muted">
            {data.view.workflow.version} · on {data.view.eval} ({data.view.recordId}) · started
            {data.view.workflow.started} · run {data.view.runId}
          </p>
        </div>
        <div class="row head-kpis">
          <div><p class="kpi-n">{data.view.duration}</p><p class="kpi-k">duration</p></div>
          <div><p class="kpi-n fail-text">{data.view.passed}/{data.view.total}</p><p class="kpi-k">tasks passed</p></div>
          <a class="control" href={backHref} aria-label="Close workflow and return to evaluations">✕</a>
        </div>
      </header>
      <div class="row stages-row">
        <span class="mono muted head-k">Stages</span>
        <div class="stages">
          {#each data.view.stages as stage, i (stage.name)}
            {#if i > 0}<span class="arrow" aria-hidden="true">▸</span>{/if}
            <span class="stage" data-tone={stage.tone}>{stage.name}<br /><small class="muted">{stage.state}</small></span>
          {/each}
        </div>
      </div>
      <div class="drawer-columns">
        <Panel title="Tasks">
          <div class="task-list">
            {#each data.view.tasks as task (task.name)}
              <a
                class="task"
                href={taskHref(task.name)}
                aria-current={data.view.selected.name === task.name ? 'true' : undefined}
              >
                <span><strong>{task.name}</strong></span>
                <Badge tone={task.status.tone}>{task.status.label}</Badge>
                <span class="mono muted">{task.method} · {task.stage}</span>
                <span class="mono task-score">{task.score}</span>
              </a>
            {/each}
          </div>
        </Panel>
        <Panel variant="raised" title={data.view.selected.name}>
          {#snippet head()}
            <Badge tone={data.view.selected.status.tone}>{data.view.selected.status.label}</Badge>
            <span class="mono muted">task {data.view.tasks.indexOf(data.view.selected) + 1} of {data.view.total} · {data.view.selected.stage}</span>
          {/snippet}
          <div class="stack">
            <div class="kv task-facts">
              <span class="k">type</span><span class="mono">{data.view.selected.detail.type}</span>
              <span class="k">stage</span><span class="mono">{data.view.selected.stage}</span>
              <span class="k">method</span><span class="mono">{data.view.selected.method}</span>
              <span class="k">weight</span><span class="mono">{data.view.selected.detail.weight}</span>
              <span class="k">started</span><span class="mono">{data.view.selected.detail.started}</span>
              <span class="k">duration</span><span class="mono">{data.view.selected.detail.duration}</span>
              <span class="k">attempt</span><span class="mono">{data.view.selected.detail.attempt}</span>
              <span class="k">cost</span><span class="mono">{data.view.selected.detail.cost}</span>
            </div>
            {#if data.view.selected.detail.scoreValue !== null && data.view.selected.detail.threshold !== null}
              <div class="score">
                <div>
                  <p class={`n ${data.view.selected.status.tone === 'danger' ? 'fail' : 'pass'}`}>{data.view.selected.score}</p>
                  <p class="kpi-k">score</p>
                </div>
                <div class="score-bar" role="img" aria-label={data.view.selected.detail.comparison}>
                  <span
                    class={`fill ${data.view.selected.status.tone === 'danger' ? '' : 'pass'}`}
                    style={`width:${scorePct(data.view.selected.detail)}%`}
                  ></span>
                  <span class="tick" style={`left:${tickPct(data.view.selected.detail)}%`}></span>
                </div>
                {#if data.view.selected.detail.history.length}
                  <div>
                    <div class="history" role="img" aria-label={`Last ${data.view.selected.detail.history.length} outcomes: ${data.view.selected.detail.history.join(', ')}`}>
                      {#each data.view.selected.detail.history as outcome, i (i)}
                        <span class={outcome === 'fail' ? 'fail' : ''}></span>
                      {/each}
                    </div>
                    <p class="kpi-k">last {data.view.selected.detail.history.length}</p>
                  </div>
                {/if}
              </div>
              <p class="mono muted">{data.view.selected.detail.comparison}</p>
            {:else}
              <p class="mono muted">{data.view.selected.detail.comparison}</p>
            {/if}
            {#if data.view.selected.detail.judgeExplanation}
              <div>
                <p class="mono muted head-k">Judge explanation</p>
                <div class="notice"><p>{data.view.selected.detail.judgeExplanation}</p></div>
              </div>
            {/if}
            <div>
              <p class="mono muted head-k">
                Values
                {#if data.view.selected.detail.withheldReason}
                  · actuals withheld
                {:else if data.view.selected.detail.actual}
                  · actuals shown — you are authorized · others see withheld
                {/if}
              </p>
              <div class="values">
                <div class="val">
                  <p class="mono muted">expected</p>
                  {#each data.view.selected.detail.expected as line (line)}<p>{line}</p>{/each}
                </div>
                {#if data.view.selected.detail.actual}
                  <div class={`val ${data.view.selected.status.tone === 'danger' ? 'fail' : ''}`}>
                    <p class="mono muted">actual</p>
                    {#each data.view.selected.detail.actual as line (line)}<p>{line}</p>{/each}
                  </div>
                {:else if data.view.selected.detail.withheldReason}
                  <StateBlock state="unauthorized" title="Actual values withheld" detail={data.view.selected.detail.withheldReason} />
                {/if}
              </div>
            </div>
            {#if data.view.selected.detail.traceLink}
              <a
                class="control trace-link"
                href={`${base}/observe/traces/${data.view.selected.detail.traceLink.href}${scopeQuery(data.scope)}`}
                >→ {data.view.selected.detail.traceLink.label} · filters preserved</a
              >
            {/if}
          </div>
        </Panel>
      </div>
    </section>
  {/if}
</div>

<style>
  .drawer-head {
    align-items: flex-start;
    margin-bottom: 14px;
  }
  .drawer-head h2 {
    margin: 2px 0 4px;
  }
  .head-k {
    font: 700 9px var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 0 0 4px;
  }
  .head-kpis {
    gap: 18px;
    align-items: flex-start;
  }
  .kpi-n {
    font: 700 20px var(--font-display);
    margin: 0;
  }
  .fail-text {
    color: var(--danger-text);
  }
  .kpi-k {
    font: 700 9px var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 2px 0 0;
  }
  .stages-row {
    margin-bottom: 16px;
    align-items: flex-start;
  }
  .drawer-columns {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) minmax(0, 2.2fr);
    gap: 16px;
    align-items: start;
  }
  .task-score {
    text-align: right;
  }
  .task-facts {
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  }
  .passed {
    font-size: 20px;
  }
  .trace-link {
    justify-self: start;
  }
  @media (max-width: 900px) {
    .drawer-columns {
      grid-template-columns: minmax(0, 1fr);
    }
    .task-facts {
      grid-template-columns: auto minmax(0, 1fr);
    }
  }
</style>
