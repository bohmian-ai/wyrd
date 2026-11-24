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
  import { attemptsOf, executionStates, inputWaits, missingInputs, requirementAttempts, requirementState, sameProducer, settled, type Part, type VerificationExecutionProps } from '../execution.js';
  import { fmtBytes, fmtCardRef, fmtDuration, fmtShortId, fmtYaml } from '../format.js';
  import type { ExecutionInput } from '../schemas.js';
  import Code from './Code.svelte';
  import ActionButton from './ActionButton.svelte';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';
  import StatusMark from './StatusMark.svelte';
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

  const DRIFT_WORDS: Record<CardTypes.DriftVerdict, string> = { NoDrift: 'no drift', Drift: 'drift', Inconclusive: 'inconclusive' };
  const WORDS = { input: 'Input', step: 'Step', req: 'Requirement', task: 'Task' } as const;
  /** The most characters of one JSON or text value a preview draws. */
  const PREVIEW = 65_536;

  const data = $derived(result.state === 'populated' ? result.data : undefined);
  const definition = $derived(data?.definition.state === 'populated' ? data.definition.data : undefined);
  const items = $derived(data?.invocations?.state === 'populated' ? data.invocations.data.items : []);
  const outputs = $derived(data?.publications?.state === 'populated' ? data.publications.data.items : []);
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

  /** Steps that recorded an invocation, in recorded order; the only steps known when the definition is unavailable. */
  const ran = $derived([...new Set(items.filter((each) => each.state !== 'requirement').map((each) => each.producer.step_id))]);
  /** Every approval occurrence the execution reached, in step order, read from its status alone. */
  const approvals = $derived(data?.status.approvals ?? []);
  /** Steps known without the definition: those that recorded an invocation, then those that reached an approval. */
  const reached = $derived([...new Set([...ran, ...approvals.map((each) => each.step_id)])]);
  /** Whether `step` of this unfinished execution waits for a decision. */
  const awaiting = (step: string) => !!data && !settled(data.status.status) && approvals.some((each) => each.step_id === step && each.status === 'pending');
  /** Inputs this unfinished execution started without and still waits for. */
  const missing = $derived(data ? missingInputs(data.status) : []);
  /** The inputs each task reports waiting for, by step or requirement ID. */
  const waits = $derived(data ? inputWaits(data.status) : new Map<string, readonly string[]>());
  /** Requirements judged in the result or by a settled invocation, each once, in recorded order; the only requirements known when the definition is unavailable. */
  const judgedIds = $derived([...new Set([...(data?.status.result?.outcome.requirements.map((each) => each.id) ?? []), ...items.flatMap((each) => (each.state === 'requirement' ? [each.producer.step_id] : []))])]);
  /** Waiting tasks known by no recorded invocation, approval or judgment; without the definition their kind is unknown. */
  const waitingOnly = $derived([...waits.keys()].filter((task) => !reached.includes(task) && !judgedIds.includes(task)));
  const attempts = $derived(type === 'step' ? attemptsOf(items, id) : []);
  const attempt = $derived(attempts.find((each) => selection.producer && sameProducer(each.producer, selection.producer)) ?? attempts.at(-1));
  const produced = $derived(attempt ? outputs.filter((each) => sameProducer(each.producer, attempt.producer)) : []);
  const judgments = $derived(type === 'req' ? (data?.status.result?.outcome.requirements.filter((each) => each.id === id) ?? []) : []);
  /** Without a judgment in the result, the requirement's settled invocations are what was recorded of it. */
  const progress = $derived(type === 'req' && !judgments.length ? requirementAttempts(items, id) : []);
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
  /** The host's answer to the last decision, shown once for the page. */
  const decided = $derived(actions['approval.decide']);
  const off = (name: string) => !onaction || act(name)?.state === 'pending' || act(name)?.state === 'unavailable';

  /** The reason typed for each pending occurrence, by approval id. */
  let reasons = $state<Record<string, string>>({});
  /** The occurrence the last decision was sent for: the host's answer belongs to it alone, whatever is selected since. */
  let decidedFor = $state<string>();
  function decide(occurrence: CardTypes.ApprovalOccurrenceState, decision: CardTypes.ApprovalDecision) {
    if (!data || off('approval.decide')) return;
    decidedFor = occurrence.approval_id;
    const reason = reasons[occurrence.approval_id]?.trim();
    onaction?.({ name: 'approval.decide', input: { execution_id: data.status.execution_id, approval_id: occurrence.approval_id, decision, ...(reason ? { reason } : {}) } });
  }
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
              <td><StatusMark value={DRIFT_WORDS[feature.verdict]} /></td>
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

{#snippet approval(each: CardTypes.ApprovalOccurrenceState)}
  {@const mine = decidedFor === each.approval_id}
  <div class="approval" data-approval={each.approval_id}>
    <div class="approval-head">
      <span class="label">Human approval</span>
      <span data-approval-status><StatusMark value={each.status === 'pending' ? 'waiting' : each.status} label={each.status === 'pending' ? 'pending' : undefined} badge /></span>
    </div>
    <p class="prose">{each.message}</p>
    <dl class="dkv">
      <dt>Occurrence</dt><dd>{each.approval_id}</dd>
      <dt>For</dt><dd>{[input.executionId, `step ${each.step_id}`, each.item == null ? '' : `item ${each.item}`, each.iteration == null ? '' : `iteration ${each.iteration}`].filter(Boolean).join(' · ')}</dd>
      {#if each.status === 'pending'}<dt>Expires</dt><dd>with this execution · at its deadline, on cancellation, or when newer work supersedes it</dd>{/if}
      {#if each.actor_id}<dt>Decided</dt><dd>{each.status} by {each.actor_id} at {each.decided_at}{each.reason ? ` · “${each.reason}”` : ''}</dd>{/if}
    </dl>
    {#if each.status === 'pending' && data && !settled(data.status.status)}
      {#if onaction && act('approval.decide')?.state !== 'unavailable' && act('approval.decide')?.state !== 'denied'}
        <label class="field">Reason (optional)
          <textarea rows="2" maxlength="4096" bind:value={reasons[each.approval_id]} disabled={off('approval.decide')}></textarea>
        </label>
        <div class="decide">
          <ActionButton variant="primary" label="Approve" disabled={off('approval.decide')} onclick={() => decide(each, 'approved')} />
          <ActionButton label="Reject" disabled={off('approval.decide')} onclick={() => decide(each, 'rejected')} />
          {#if mine && act('approval.decide')?.state === 'pending'}<span class="small muted" role="status">Submitting for {each.approval_id}…</span>{/if}
        </div>
      {:else if act('approval.decide')?.state === 'denied'}
        <p class="note" data-viewer-note>You can read this approval. Approving or rejecting needs approve on this Verifier and read on its Repository.</p>
      {:else}
        <p class="note" data-viewer-note>You can read this approval. This view cannot submit a decision; open the execution in Wyrd to approve or reject.</p>
      {/if}
    {/if}
  </div>
{/snippet}

{#if result.state === 'populated' && data}
  {@const status = data.status}
  <div class="execution">
    <Panel title="Execution" note={status.execution_id}>
      {#snippet tools()}
        <button type="button" class="tool" aria-pressed={selection.node === 'outputs'} onclick={() => select({ node: 'outputs' })}>All retained outputs</button>
        <button type="button" class="tool" aria-pressed={!!selection.details} onclick={() => select({ ...selection, details: !selection.details })}>{selection.details ? 'Hide completed details' : 'Load completed details'}</button>
        {#if !settled(status.status)}
          <ActionButton label="Cancel execution" state={act('execution.cancel')} disabled={!onaction} onclick={() => off('execution.cancel') || onaction?.({ name: 'execution.cancel', input: { execution_id: status.execution_id } })} />
        {:else}
          <ActionNote state={act('execution.cancel')} />
        {/if}
      {/snippet}
      <dl class="summary">
        <div class="fact"><dt>Status</dt><dd data-execution-status={status.status}>
            <StatusMark value={status.status} badge />
            {#if missing.length}<span class="tag" data-blocked="awaiting_input">waiting for input <b>{missing.join(', ')}</b></span>{/if}
            {#if !settled(status.status) && approvals.some((each) => each.status === 'pending')}<span class="tag" data-blocked="awaiting_approval">waiting for approval</span>{/if}
          </dd></div>
        <div class="fact">
          <dt>Verdict</dt>
          <dd data-verdict={status.result?.outcome.verdict ?? ''}>
            {#if status.result}<StatusMark value={status.result.outcome.verdict} badge />{:else}<span class="muted">{settled(status.status) ? 'no recorded verdict' : 'none while running'}</span>{/if}
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
        {#if status.pull_request}
          {@const pull = status.pull_request}
          {@const repository = data.repository}
          <div class="fact">
            <dt>Pull request</dt>
            <dd>
              {#if repository}
                <button type="button" class="open" disabled={off('pull_request.open')} title="Open review set {pull.generation}" onclick={() => onaction?.({ name: 'pull_request.open', input: { repository, number: pull.number, generation: pull.generation } })}>{repository}#{pull.number}</button>
              {:else}
                #{pull.number}
              {/if}
              <span class="muted">head {pull.head.sha.slice(0, 7)}</span>
            </dd>
          </div>
        {/if}
        {@render fact('Run', status.result?.run_id)}
        {@render fact('Session', status.group_id)}
        {@render fact('Owner Card', status.owner_card_uid)}
      </dl>
      {#if status.error}
        <p class="failure" role="note"><span aria-hidden="true">✕</span> {status.error.code}: {status.error.message}</p>
      {/if}
      {#if definition?.spec.description}<p class="description">{definition.spec.description}</p>{/if}
      <ActionNote state={act('card.open')} />
    </Panel>
    {#if decided?.state === 'success'}
      <p class="banner ok" role="status" data-decision-notice>{decided.message}</p>
    {:else if decided?.state === 'error' || decided?.state === 'denied'}
      <p class="banner warn" role="status" data-decision-notice>{decided.problem.title} <span class="mono small muted">{decided.problem.code}</span></p>
    {/if}

    {#if definition && states}
      {#key fmtCardRef(definition.ref)}
        <VerifierGraph graph={definition.graph} {states} bind:selected={() => (selection.node?.includes(':') ? selection.node : ''), (key) => select({ node: key || 'none' })} />
      {/key}
    {:else}
      <Panel title="Graph" note="The Verifier this execution ran">
        {@render refused(data.definition, 'Verifier definition')}
        {#if judgedIds.length + reached.length + waitingOnly.length}
          <div class="recorded" role="group" aria-label="Recorded without the definition">
            <p class="hint">Recorded requirements and invocations stay selectable. Declared order and edges need the definition.</p>
            {#each judgedIds as requirement (requirement)}
              {@const mark = requirementState(status, items, requirement)}
              <button type="button" class="tool" aria-pressed={selection.node === `req:${requirement}`} onclick={() => select({ node: `req:${requirement}` })}><span class="label">Requirement</span> {requirement} <span class="muted">{mark.word ?? mark.value.replace('_', ' ')}</span></button>
            {/each}
            {#each reached as step (step)}
              {@const last = attemptsOf(items, step).at(-1)}
              <button type="button" class="tool" aria-pressed={selection.node === `step:${step}`} onclick={() => select({ node: `step:${step}` })}
                ><span class="label">Step</span> {step}
                {#if awaiting(step)}<StatusMark value="awaiting_approval" />{:else if last}<span class="muted">{recorded(last)}</span>{/if}</button
              >
            {/each}
            {#each waitingOnly as task (task)}
              <button type="button" class="tool" aria-pressed={selection.node === `task:${task}`} onclick={() => select({ node: `task:${task}` })}
                ><span class="label">Task</span> {task} <StatusMark value="awaiting_input" /> <b class="mono">{waits.get(task)?.join(', ')}</b></button
              >
            {/each}
          </div>
        {/if}
      </Panel>
    {/if}
    {#if data.invocations}{@render refused(data.invocations, 'Recorded progress')}{/if}
    {#if data.invocations?.state === 'populated'}
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
    {#if data.publications && data.publications.state !== 'empty'}{@render refused(data.publications, 'Retained outputs')}{/if}
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
        {:else if data.publications?.state === 'populated'}
          <p class="hint">No output is retained for this execution.</p>
        {:else if data.publications?.state === 'empty'}
          <p class="hint">{data.publications.reason}</p>
        {/if}
        {#if data.publications?.state === 'populated' && data.publications.data.next_cursor}
          {@const cursor = data.publications.data.next_cursor}
          <button type="button" class="open more" disabled={off('execution.page')} onclick={() => onaction?.({ name: 'execution.page', input: { publications: cursor } })}>Next page of outputs</button>
        {/if}
      </Panel>
    {:else if type && id}
      <Panel title="{WORDS[type]} {id}" note={type === 'req' ? 'Recorded judgment' : type === 'step' ? 'Recorded invocations' : type === 'task' ? 'Reported wait' : 'Recorded input'}>
        {#if type === 'input' && missing.includes(id)}
          {@const readers = [...waits].filter(([, slots]) => slots.includes(id)).map(([task]) => (definition?.graph.requirements.some((each) => each.id === task) ? `${task} is judged` : `${task} starts`))}
          <p class="note box waitnote" data-waiting-input>Not provided yet. The execution started without it{#if readers.length}; {readers.join(' and ')} when it arrives{/if}.</p>
        {/if}
        {#if type === 'step'}
          {#each approvals.filter((each) => each.step_id === id) as each (each.approval_id)}
            {@render approval(each)}
          {/each}
        {/if}
        {#if type !== 'input' && waits.has(id) && !attempts.length}
          <p class="note box waitnote" data-waiting-input>Waiting for input <b class="mono">{waits.get(id)?.join(', ')}</b>. The execution started without it; this {type === 'req' ? 'requirement is judged' : type === 'step' ? 'step starts' : 'task runs'} when the evidence is provided. Nothing here is judged until then.</p>
        {/if}
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
              {:else if data.publications?.state === 'populated'}
                <p class="hint">This invocation retained no output.</p>
              {:else if data.publications?.state === 'empty'}
                <p class="hint">{data.publications.reason}</p>
              {/if}
            </div>
            {#if loaded?.publication && selection.output}{@render publication(loaded.publication, id)}{/if}
          {:else if data.invocations?.state === 'populated'}
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
                <div class="fact"><dt>Outcome</dt><dd><StatusMark value={each.outcome} /></dd></div>
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
            {#each progress as each (where(each.producer))}
              <div class="judgment" data-outcome={each.outcome} data-producer={where(each.producer)}>
                <dl>
                  <div class="fact"><dt>Outcome</dt><dd><StatusMark value={each.outcome} /></dd></div>
                  {@render fact('Reason', each.reason)}
                  {@render fact('Invocation', where(each.producer))}
                  {@render fact('Disposition', each.speculation === 'none' ? undefined : each.speculation)}
                </dl>
              </div>
            {:else}
              <p class="hint">{settled(status.status) ? 'No judgment of this requirement was recorded.' : 'Not judged yet.'}</p>
            {/each}
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
                  <StatusMark value={member.status.data.status} />
                  {#if member.status.data.result}<StatusMark value={member.status.data.result.outcome.verdict} badge />{/if}
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
  .approval {
    display: flex;
    flex-direction: column;
    gap: 10px;
    border: 1px solid var(--warn);
    border-left-width: 2px;
    border-radius: var(--r);
    padding: 12px 14px;
    background: var(--bg);
  }
  .approval-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .decide {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .approval-head [data-approval-status] {
    margin-left: auto;
  }
  .prose {
    margin: 0;
    font-size: 13px;
  }
  .dkv {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 8px 14px;
    margin: 0;
    font: 12px/1.5 var(--font-mono);
  }
  .dkv dt,
  .dkv dd {
    font: 12px/1.5 var(--font-mono);
    letter-spacing: normal;
    text-transform: none;
  }
  .dkv dt {
    color: var(--muted);
  }
  .dkv dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .approval .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .note {
    max-width: 80ch;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .note.box {
    border: 1px dashed var(--border);
    border-radius: var(--r);
    padding: 8px 10px;
  }
  .waitnote {
    border-color: var(--warn);
    color: var(--text);
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 4px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
    padding: 0 6px;
    color: var(--muted);
    font: 11px/17px var(--font-mono);
    white-space: nowrap;
  }
  .tag b {
    color: var(--text);
    font-weight: 400;
  }
  .banner {
    margin: 0;
    border: 1px solid var(--border);
    border-left: 2px solid var(--declare);
    border-radius: var(--r);
    padding: 8px 12px;
    background: var(--surface);
    font-size: 13px;
  }
  .banner.ok {
    border-left-color: var(--ok);
  }
  .banner.warn {
    border-left-color: var(--warn);
  }
  .small {
    font-size: 12px;
  }
  @media (max-width: 900px) {
    .dkv {
      grid-template-columns: minmax(0, 1fr);
      gap: 2px;
    }
    .dkv dd {
      margin-bottom: 8px;
    }
  }
  textarea {
    min-height: 52px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 6px 8px;
    background: var(--surface);
    color: var(--text);
    font: 13px/1.5 var(--font-sans);
    resize: vertical;
  }
  textarea:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .recorded {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .muted {
    color: var(--muted);
  }
  .recorded .hint {
    flex-basis: 100%;
    margin: 0;
  }
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
