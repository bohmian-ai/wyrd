<!--
  One exact verification execution: its lifecycle status beside its verdict,
  the graph of the Verifier it ran marked with recorded state, and an
  inspector for the selected step, requirement, input or retained output.
  Every part renders or refuses on its own; a refused part never hides
  another. Selection, paging, opening and cancelling are host actions.
-->
<script lang="ts">
  import type { CardTypes } from 'wyrd-ai';
  import type { ActionProps } from '../contract.js';
  import { attemptsOf, executionStates, sameProducer, settled, type Part, type VerificationExecutionProps } from '../execution.js';
  import { fmtBytes, fmtCardRef, fmtDuration, fmtShortId, fmtYaml } from '../format.js';
  import type { ExecutionInput } from '../schemas.js';
  import Code from './Code.svelte';
  import ActionButton from './ActionButton.svelte';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';
  import StatusPill, { type Tone } from './StatusPill.svelte';
  import VerdictPill from './VerdictPill.svelte';
  import VerifierGraph from './VerifierGraph.svelte';

  let { input, result, actions = {}, onaction }: VerificationExecutionProps & ActionProps = $props();

  type Selection = Pick<ExecutionInput, 'node' | 'producer' | 'output' | 'details'>;
  type Producer = CardTypes.Producer;

  // The host owns selection through `input`; a choice shows at once and holds until the host answers with new props.
  let chosen = $state.raw<{ readonly from: ExecutionInput; readonly value: Selection }>();
  // While the host is still answering a selection, an older answer arriving does not replace the newer choice.
  const selection = $derived<Selection>(
    chosen && (chosen.from === input || actions['execution.select']?.state === 'pending') ? chosen.value : { node: input.node, producer: input.producer, output: input.output, details: input.details }
  );
  /** Whether the loaded selection-specific parts (output body, members, accepted input, drift context) belong to the selection shown. */
  const current = $derived(
    selection.node === input.node &&
      selection.output === input.output &&
      !selection.producer === !input.producer &&
      (!selection.producer || !input.producer || sameProducer({ step_id: '', ...selection.producer }, input.producer))
  );
  const loaded = $derived(current && result.state === 'populated' ? result.data : undefined);
  function select(next: Selection) {
    const value = Object.fromEntries(Object.entries({ details: selection.details, ...next }).filter(([, each]) => each !== undefined && each !== false)) as Selection;
    chosen = { from: input, value };
    onaction?.({ name: 'execution.select', input: value });
  }

  const STATUS_TONES: Record<CardTypes.VerificationExecutionStatus, Tone> = {
    pending: 'neutral',
    running: 'running',
    retrying: 'running',
    waiting: 'warn',
    completed: 'ok',
    cancelled: 'neutral',
    timed_out: 'danger',
    errored: 'danger'
  };
  const OUTCOME_TONES: Record<CardTypes.RequirementOutcome, Tone> = { passed: 'ok', failed: 'danger', inconclusive: 'warn', not_applicable: 'neutral' };
  const DRIFT_TONES: Record<CardTypes.DriftVerdict, Tone> = { NoDrift: 'ok', Drift: 'danger', Inconclusive: 'warn' };
  const DRIFT_WORDS: Record<CardTypes.DriftVerdict, string> = { NoDrift: 'no drift', Drift: 'drift', Inconclusive: 'inconclusive' };
  const WORDS = { input: 'Input', step: 'Step', req: 'Requirement' } as const;
  /** The most characters of one JSON or text value a preview draws. */
  const PREVIEW = 65_536;

  const data = $derived(result.state === 'populated' ? result.data : undefined);
  const definition = $derived(data?.definition.state === 'populated' ? data.definition.data : undefined);
  const items = $derived(data?.invocations.state === 'populated' ? data.invocations.data.items : []);
  const outputs = $derived(data?.publications.state === 'populated' ? data.publications.data.items : []);
  const states = $derived(data && definition ? executionStates(definition.graph, data.status, items) : undefined);
  const declared = $derived(definition ? definition.graph.requirements.length + definition.graph.steps.reduce((sum, step) => sum + step.requirements.length, 0) : 0);
  const judged = $derived(new Set(data?.status.result?.outcome.requirements.map((each) => each.id)).size);
  const [type, id] = $derived((selection.node ?? '').split(/:(.*)/s) as [keyof typeof WORDS | 'outputs' | '', string]);

  /** The authored block of the selected node, exactly as the spec carries it. */
  const block = $derived.by(() => {
    if (!definition || !id) return undefined;
    const steps = definition.spec.steps ?? [];
    if (type === 'input') return definition.spec.inputs?.[id] === undefined ? undefined : { [id]: definition.spec.inputs[id] };
    if (type === 'step') return steps.find((step) => step.id === id);
    return [...(definition.spec.requirements ?? []), ...steps.flatMap((step) => step.requirements ?? [])].find((each) => each.id === id);
  });

  const attempts = $derived(type === 'step' ? attemptsOf(items, id) : []);
  const attempt = $derived(attempts.find((each) => selection.producer && sameProducer(each.producer, selection.producer)) ?? attempts.at(-1));
  const produced = $derived(attempt ? outputs.filter((each) => sameProducer(each.producer, attempt.producer)) : []);
  const judgments = $derived(type === 'req' ? (data?.status.result?.outcome.requirements.filter((each) => each.id === id) ?? []) : []);
  const completed = $derived(data?.details?.state === 'populated' ? data.details.data.outcome : undefined);

  const where = (producer: Producer) =>
    [producer.item_index == null ? '' : `item ${producer.item_index}`, producer.iteration_index == null ? '' : `iteration ${producer.iteration_index}`, `attempt ${producer.attempt}`].filter(Boolean).join(' · ');
  const recorded = (each: CardTypes.ExecutionInvocation) => (each.state === 'action' ? each.status : each.state === 'requirement' ? each.outcome.replace('_', ' ') : 'started');
  const pick = (producer: Producer) => ({ item_index: producer.item_index ?? null, iteration_index: producer.iteration_index ?? null, attempt: producer.attempt });
  const locator = (value: CardTypes.Locator) => fmtYaml(value).trim().replace(/\n\s*/g, ' ');

  /** A value as bounded JSON text; `cut` says how much was left out. */
  function bounded(value: unknown) {
    const whole = typeof value === 'string' ? value : (JSON.stringify(value, null, 2) ?? 'null');
    return { text: whole.slice(0, PREVIEW), cut: Math.max(0, whole.length - PREVIEW) };
  }

  /** The check a requirement declares in the exact Verifier this execution ran, whole-Verifier or attached to a step. */
  const declaredCheck = (id: string) => [...(definition?.spec.requirements ?? []), ...(definition?.spec.steps ?? []).flatMap((step) => step.requirements ?? [])].find((each) => each.id === id)?.check;

  /** The exact Agent Card a requirement's check declares as its judge, when it names one rather than declaring the Agent inline. */
  function judge(id: string): CardTypes.CardRef | undefined {
    const check = declaredCheck(id);
    const agent = check?.kind === 'agent' ? check.agent : undefined;
    return typeof agent === 'object' && 'kind' in agent && agent.kind === 'Agent' && 'version' in agent ? (agent as CardTypes.CardRef) : undefined;
  }

  const act = (name: string) => actions[name];
  const off = (name: string) => !onaction || act(name)?.state === 'pending' || act(name)?.state === 'unavailable';
</script>

{#snippet refused(part: Part<unknown>, what: string)}
  {#if part.state === 'empty'}
    <p class="hint" data-part={what}>{part.reason}</p>
  {:else if part.state !== 'populated'}
    <div data-part={what}><EmptyState state={part.state} title="{what}: {part.problem.title}" problem={part.problem} /></div>
  {/if}
{/snippet}

{#snippet value(label: string, content: unknown, code = typeof content !== 'string')}
  {@const shown = bounded(content)}
  <div class="field">
    <span class="label">{label}</span>
    {#if code}<Code text={shown.text} />{:else}<pre>{shown.text}</pre>{/if}
    {#if shown.cut}<p class="hint">Preview stops at {fmtBytes(PREVIEW)}; {fmtBytes(shown.cut)} more is retained.</p>{/if}
  </div>
{/snippet}

{#snippet fact(label: string, text: string | null | undefined)}
  {#if text}<div class="fact"><dt>{label}</dt><dd>{text}</dd></div>{/if}
{/snippet}

{#snippet drift(report: CardTypes.DriftReport, requirement: string)}
  {@const check = declaredCheck(requirement)}
  {@const context = loaded?.drift}
  {@const history = context?.state === 'populated' ? context.data.history : []}
  {@const features = Object.values(report.features).sort((a, b) => Number(b.verdict === 'Drift') - Number(a.verdict === 'Drift') || a.feature.localeCompare(b.feature))}
  <div class="field" data-drift>
    <span class="label">Drift</span>
    <dl>
      {@render fact('Method', report.method)}
      {@render fact('Recorded verdict', DRIFT_WORDS[report.verdict])}
      {#if context?.state === 'populated'}
        {@render fact('Window', context.data.window_start && context.data.window_end ? `${context.data.window_start} → ${context.data.window_end}` : 'no window recorded')}
      {/if}
    </dl>
    {#if context}{@render refused(context, 'Window and history')}{/if}
    <!-- The baseline is what the exact Verifier declares; a current Card status is never read for it. -->
    {#if check?.kind === 'drift'}
      <div data-declared>{@render value('Declared baseline and condition', fmtYaml({ signal: check.signal, ...(check.profile ? { profile: check.profile } : {}), condition: check.condition }).trim(), true)}</div>
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="scroll" role="region" aria-label="Drift features" tabindex="0">
      <table>
        <thead><tr><th>Feature</th><th>Verdict</th><th>Score</th><th>Threshold</th><th>Recorded</th></tr></thead>
        <tbody>
          {#each features as feature (feature.feature)}
            {@const points = history.map((point) => ({ score: point.scores[feature.feature], current: point.execution_id === input.executionId })).filter((point) => typeof point.score === 'number')}
            {@const top = Math.max(feature.threshold ?? 0, ...points.map((point) => point.score as number), Number.MIN_VALUE)}
            {@const y = (score: number) => 22 - (score / top) * 20}
            <tr data-feature={feature.feature}>
              <td>{feature.feature}</td>
              <td><StatusPill tone={DRIFT_TONES[feature.verdict]} label={DRIFT_WORDS[feature.verdict]} /></td>
              <td>{feature.score ?? '—'}</td>
              <td>{feature.threshold ?? '—'}</td>
              <td>
                {#if points.length > 1}
                  <svg class="spark" viewBox="0 0 120 24" width="120" height="24" role="img" aria-label="{feature.feature}: {points.length} recorded scores, this window marked">
                    {#if feature.threshold !== null}<line class="threshold" x1="0" x2="120" y1={y(feature.threshold)} y2={y(feature.threshold)} />{/if}
                    <polyline points={points.map((point, index) => `${4 + (index / (points.length - 1)) * 112},${y(point.score as number)}`).join(' ')} />
                    {#each points as point, index (index)}
                      {#if point.current}<circle class="current" cx={4 + (index / (points.length - 1)) * 112} cy={y(point.score as number)} r="3" />{/if}
                    {/each}
                  </svg>
                {:else}
                  <span class="muted">{context?.state === 'populated' ? 'no earlier result' : '—'}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/snippet}

{#snippet publication(part: Part<CardTypes.Publication>, step: string)}
  <div class="field" data-preview={selection.output}>
    <span class="label">Output {selection.output}</span>
    {@render refused(part, 'Output')}
    {#if part.state === 'populated'}
      {@const body = part.data}
      {#if body.kind === 'json'}
        {@render value('JSON', body.value)}
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="scroll" role="region" aria-label="Files of {body.name}" tabindex="0">
          <table>
            <thead><tr><th>Path</th><th>Size</th><th>SHA-256</th><th></th></tr></thead>
            <tbody>
              {#each body.manifest.files as file (file.path)}
                <tr>
                  <td>{file.path}</td>
                  <td>{fmtBytes(file.size_bytes)}</td>
                  <td title={file.sha256}>{fmtShortId(file.sha256)}</td>
                  <td>
                    <button
                      type="button"
                      class="open"
                      disabled={off('publication.download')}
                      onclick={() => onaction?.({ name: 'publication.download', input: { step_id: step, name: body.name, producer: pick(body.producer), ...(body.kind === 'directory' ? { path: file.path } : {}) } })}
                    >Download</button>
                  </td>
                </tr>
              {:else}
                <tr><td colspan="4" class="muted">No file is retained under this name.</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
        <ActionNote state={act('publication.download')} />
        {#if loaded?.text}
          <!-- Text is shown as text: Markdown is its source, and markup never becomes elements. -->
          <div data-text>
            {@render refused(loaded.text, 'File text')}
            {#if loaded.text.state === 'populated'}
              {#if loaded.text.data.text === null}
                <p class="hint">This file is not UTF-8 text. Download it to read it.</p>
              {:else}
                {@render value('Text', loaded.text.data.text)}
              {/if}
            {/if}
          </div>
        {/if}
      {/if}
    {/if}
  </div>
{/snippet}

{#if result.state === 'populated' && data}
  {@const status = data.status}
  <div class="execution">
    <Panel title="Execution" note={status.execution_id}>
      {#snippet tools()}
        <button type="button" class="tool" aria-pressed={selection.node === 'outputs'} onclick={() => select({ node: 'outputs' })}>All retained outputs ({outputs.length}{data.publications.state === 'populated' && data.publications.data.next_cursor ? '+' : ''})</button>
        <button type="button" class="tool" aria-pressed={!!selection.details} onclick={() => select({ ...selection, details: !selection.details })}>{selection.details ? 'Hide completed details' : 'Load completed details'}</button>
        {#if !settled(status.status)}
          <ActionButton label="Cancel execution" state={act('execution.cancel')} disabled={!onaction} onclick={() => off('execution.cancel') || onaction?.({ name: 'execution.cancel', input: { execution_id: status.execution_id } })} />
        {:else}
          <ActionNote state={act('execution.cancel')} />
        {/if}
      {/snippet}
      <dl class="summary">
        <div class="fact"><dt>Status</dt><dd data-execution-status={status.status}><StatusPill tone={STATUS_TONES[status.status]} label={status.status.replace('_', ' ')} /></dd></div>
        <div class="fact">
          <dt>Verdict</dt>
          <dd data-verdict={status.result?.outcome.verdict ?? ''}>
            {#if status.result}<VerdictPill verdict={status.result.outcome.verdict} />{:else}<span class="muted">no verdict recorded</span>{/if}
          </dd>
        </div>
        {#if definition}<div class="fact"><dt>Judged</dt><dd>{judged} of {declared} requirements</dd></div>{/if}
        <div class="fact">
          <dt>Verifier</dt>
          <dd>
            {#if definition}
              <button type="button" class="open" disabled={off('card.open')} title={fmtCardRef(definition.ref)} onclick={() => onaction?.({ name: 'card.open', input: { card_ref: definition.ref } })}>{definition.ref.name}@{definition.ref.version}</button>
            {:else}
              <span class="muted">not available</span>
            {/if}
          </dd>
        </div>
        {@render fact('Run', status.result?.run_id)}
        {@render fact('Session', status.group_id)}
        {@render fact('Owner Card', status.owner_card_uid)}
        {#if status.waiting?.length}{@render fact('Waiting for', status.waiting.join(', '))}{/if}
      </dl>
      {#if status.error}
        <p class="failure" role="note"><span aria-hidden="true">✕</span> {status.error.code}: {status.error.message}</p>
      {/if}
      {#if definition?.spec.description}<p class="description">{definition.spec.description}</p>{/if}
      <ActionNote state={act('card.open')} />
    </Panel>

    {#if definition && states}
      {#key fmtCardRef(definition.ref)}
        <VerifierGraph graph={definition.graph} {states} bind:selected={() => (selection.node?.includes(':') ? selection.node : ''), (key) => select({ node: key || 'none' })} />
      {/key}
    {:else}
      <Panel title="Graph" note="The Verifier this execution ran">
        {@render refused(data.definition, 'Verifier definition')}
      </Panel>
    {/if}
    {@render refused(data.invocations, 'Recorded progress')}
    {#if data.invocations.state === 'populated'}
      {@const page = data.invocations.data}
      {#if page.history.pending + page.history.failed > 0}
        <p class="hint" data-part="history">{page.history.pending + page.history.failed} history events are not yet retained; recorded progress and outputs below are current.</p>
      {/if}
      {#if page.next_cursor}
        {@const cursor = page.next_cursor}
        <p class="hint">
          Showing {page.items.length} recorded invocation attempts; states above cover this page only.
          <button type="button" class="open" disabled={off('execution.page')} onclick={() => onaction?.({ name: 'execution.page', input: { invocations: cursor } })}>Next page of recorded progress</button>
        </p>
      {/if}
    {/if}
    {@render refused(data.publications, 'Retained outputs')}
    {#if data.details}{@render refused(data.details, 'Completed details')}{/if}

    {#if type === 'outputs'}
      <Panel title="All retained outputs" note="Every publication, each with the invocation that produced it">
        {#if outputs.length}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="scroll flush" role="region" aria-label="Retained outputs" tabindex="0">
            <table>
              <thead><tr><th>Name</th><th>Step</th><th>Invocation</th><th>Kind</th><th>Disposition</th></tr></thead>
              <tbody>
                {#each outputs as each (`${each.producer.step_id}/${where(each.producer)}/${each.name}`)}
                  <tr>
                    <td><button type="button" class="open" onclick={() => select({ node: `step:${each.producer.step_id}`, producer: pick(each.producer), output: each.name })}>{each.name}</button></td>
                    <td>{each.producer.step_id}</td>
                    <td>{where(each.producer)}</td>
                    <td>{each.kind}</td>
                    <td>{each.speculation === 'none' ? '—' : each.speculation}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else if data.publications.state === 'populated'}
          <p class="hint">No output is retained for this execution.</p>
        {/if}
        {#if data.publications.state === 'populated' && data.publications.data.next_cursor}
          {@const cursor = data.publications.data.next_cursor}
          <button type="button" class="open more" disabled={off('execution.page')} onclick={() => onaction?.({ name: 'execution.page', input: { publications: cursor } })}>Next page of outputs</button>
        {/if}
      </Panel>
    {:else if type && id}
      <Panel title="{WORDS[type]} {id}" note={type === 'req' ? 'Recorded judgment' : type === 'step' ? 'Recorded invocations' : 'Recorded input'}>
        {#if type === 'step'}
          {#if attempts.length > 1}
            <div class="attempts" role="radiogroup" aria-label="Invocations of {id}">
              {#each attempts as each (where(each.producer))}
                <button type="button" role="radio" aria-checked={each === attempt} onclick={() => select({ node: selection.node, producer: pick(each.producer) })}>{where(each.producer)} · {recorded(each)}</button>
              {/each}
            </div>
          {/if}
          {#if attempt}
            <dl data-producer={where(attempt.producer)}>
              {@render fact('Invocation', where(attempt.producer))}
              {@render fact('Recorded', recorded(attempt))}
              {@render fact('Reason', 'reason' in attempt ? attempt.reason : undefined)}
              {@render fact('Disposition', attempt.speculation === 'none' ? undefined : attempt.speculation)}
            </dl>
            <div class="field">
              <span class="label">Outputs of this invocation</span>
              {#if produced.length}
                <table>
                  <thead><tr><th>Name</th><th>Kind</th><th></th></tr></thead>
                  <tbody>
                    {#each produced as each (each.name)}
                      <tr>
                        <td>{each.name}</td>
                        <td>{each.kind}</td>
                        <td><button type="button" class="open" aria-pressed={selection.output === each.name} onclick={() => select({ node: selection.node, producer: pick(attempt.producer), output: each.name })}>Preview</button></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {:else}
                <p class="hint">This invocation retained no output.</p>
              {/if}
            </div>
            {#if loaded?.publication && selection.output}{@render publication(loaded.publication, id)}{/if}
          {:else if data.invocations.state === 'populated'}
            <p class="hint">{settled(status.status) ? 'No invocation of this step was recorded.' : 'No invocation of this step is recorded yet.'}</p>
          {/if}
          {#if completed}
            {#each completed.steps.filter((each) => each.id === id) as step (step.id)}
              <div class="field" data-details>
                <span class="label">Completed record</span>
                <dl>
                  {@render fact('Final status', step.status)}
                  {@render fact('Attempts', String(step.attempts))}
                  {@render fact('Duration', fmtDuration(step.duration_ms))}
                  {@render fact('Reason', step.reason)}
                  {@render fact('Tool calls', step.tool_calls?.length ? String(step.tool_calls.length) : undefined)}
                </dl>
                {#if step.provenance}{@render value('Executed by', fmtYaml(step.provenance), true)}{/if}
              </div>
            {/each}
          {/if}
        {:else if type === 'req'}
          {@const agent = judge(id)}
          {#if agent}
            <p class="judge">
              <span class="label">Judged by</span>
              <button type="button" class="open" disabled={off('card.open')} title={fmtCardRef(agent)} onclick={() => onaction?.({ name: 'card.open', input: { card_ref: agent } })}>{agent.name}@{agent.version}</button>
            </p>
          {/if}
          {#each judgments as each, index (index)}
            <div class="judgment" data-outcome={each.outcome}>
              <dl>
                <div class="fact"><dt>Outcome</dt><dd><StatusPill tone={OUTCOME_TONES[each.outcome]} label={each.outcome.replace('_', ' ')} /></dd></div>
                {@render fact('Statement', each.statement)}
                {@render fact('Reason', each.reason)}
                {@render fact('Judged', [each.step ? `step ${each.step}` : '', each.item_index == null ? '' : `item ${each.item_index}`, each.iteration_index == null ? '' : `iteration ${each.iteration_index}`].filter(Boolean).join(' · '))}
              </dl>
              {#if each.actual !== undefined}{@render value('Actual', each.actual)}{/if}
              {#if each.data}{@render value('Data', each.data)}{/if}
              {#if each.citations?.length}
                <div class="field">
                  <span class="label">Evidence</span>
                  <ul class="cites">
                    {#each each.citations as cite, at (at)}<li><span class="mono">{cite.slot}</span> {locator(cite.locator)}</li>{/each}
                  </ul>
                </div>
              {/if}
              {#if each.detail?.drift}{@render drift(each.detail.drift, each.id)}{/if}
            </div>
          {:else}
            <p class="hint">{settled(status.status) ? 'No judgment of this requirement was recorded.' : 'Not judged yet.'}</p>
          {/each}
          {#if completed}
            {#each completed.requirements.filter((each) => each.id === id) as each, index (index)}
              <div class="field" data-details>
                <span class="label">Completed record</span>
                {@render value('Check', fmtYaml(each.check), true)}
                {#if each.provenance}{@render value('Executed by', fmtYaml(each.provenance), true)}{/if}
                {#if each.tool_calls?.length}{@render value('Tool calls', each.tool_calls)}{/if}
              </div>
            {/each}
          {/if}
        {:else if loaded?.members}
          <ol class="members flush" aria-label="Members of {id}">
            {#each loaded.members as member (member.execution_id)}
              <li>
                <button type="button" class="open" disabled={off('execution.open')} onclick={() => onaction?.({ name: 'execution.open', input: { execution_id: member.execution_id } })}>{member.execution_id}</button>
                {#if member.status?.state === 'populated'}
                  <StatusPill tone={STATUS_TONES[member.status.data.status]} label={member.status.data.status.replace('_', ' ')} />
                  {#if member.status.data.result}<VerdictPill verdict={member.status.data.result.outcome.verdict} />{/if}
                {:else if member.status && member.status.state !== 'empty'}
                  <span class="muted">{member.status.problem.title} ({member.status.problem.code})</span>
                {:else}
                  <span class="muted">status not read</span>
                {/if}
              </li>
            {/each}
          </ol>
          <ActionNote state={act('execution.open')} />
        {:else if loaded?.inputs}
          {@render refused(loaded.inputs, 'Accepted input')}
          {#if loaded.inputs.state === 'populated'}
            {@const accepted = loaded.inputs.data}
            {@render value('Accepted', accepted ?? null)}
            {#if accepted && typeof accepted === 'object' && 'trace_id' in accepted && typeof accepted.trace_id === 'string'}
              {@const trace = accepted.trace_id}
              <button type="button" class="open" disabled={off('record.open')} onclick={() => onaction?.({ name: 'record.open', input: { trace_id: trace } })}>Open trace {fmtShortId(trace)}</button>
              <ActionNote state={act('record.open')} />
            {/if}
          {/if}
        {/if}
        {#if block !== undefined}
          <details>
            <summary>Declaration</summary>
            <Code text={fmtYaml(block)} />
          </details>
        {/if}
      </Panel>
    {:else}
      <Panel title="Inspector">
        <p class="hint">Select an input, step or requirement in the graph, or open all retained outputs.</p>
      </Panel>
    {/if}
    <ActionNote state={act('execution.select')} />
  </div>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading execution {input.executionId}" />
{:else if result.state !== 'empty' && result.state !== 'populated'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .judge {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0;
  }
  .execution {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
    font-family: var(--font-sans);
  }
  dl {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 28px;
    margin: 0;
  }
  .fact {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  dt,
  .label {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  dd {
    margin: 0;
    font: 13px var(--font-sans);
    overflow-wrap: anywhere;
  }
  .description,
  .hint,
  .failure {
    margin: 0;
    font: 13px/1.5 var(--font-sans);
    overflow-wrap: anywhere;
  }
  .hint {
    color: var(--muted);
  }
  .failure {
    color: var(--danger);
  }
  .field,
  .judgment {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .judgment + .judgment {
    border-top: 1px solid var(--border-soft);
    padding-top: 14px;
  }
  .scroll {
    min-width: 0;
    overflow-x: auto;
  }
  /* Dense rows scroll sideways in their labelled region rather than break mid-word. */
  .scroll :is(th, td) {
    white-space: nowrap;
  }
  .tool,
  .attempts button {
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 10px;
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .tool:hover,
  .attempts button:hover {
    background: var(--surface-hover);
  }
  .tool[aria-pressed='true'],
  .attempts button[aria-checked='true'] {
    border-color: var(--declare);
    background: var(--declare-soft);
  }
  .attempts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .attempts button {
    font-family: var(--font-mono);
  }
  .more {
    align-self: flex-start;
  }
  .cites,
  .members {
    margin: 0;
    padding: 0;
    list-style: none;
    font: 12px var(--font-mono);
  }
  .members li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    border-bottom: 1px solid var(--border-soft);
    padding: 9px 16px;
  }
  .members li:last-child {
    border-bottom: 0;
  }
  .mono {
    font-family: var(--font-mono);
  }
  summary {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    cursor: pointer;
  }
  details[open] summary {
    margin-bottom: 8px;
  }
  .spark polyline {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.5;
  }
  .spark .threshold {
    stroke: var(--warn);
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }
  .spark .current {
    fill: var(--declare);
  }
</style>
