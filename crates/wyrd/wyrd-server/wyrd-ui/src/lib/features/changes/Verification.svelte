<script lang="ts">
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import { enhance } from '$app/forms';
  import type { ChangeView, ActionResult } from './types';
  let {
    view,
    csrf,
    base,
    result
  }: { view: ChangeView; csrf: string; base: string; result?: ActionResult | null } =
    $props();
  let selected = $state('');
  let change = $derived(view.change);
  let chosen = $derived(
    change.claims.flatMap((claim) => claim.checks).find((check) => check.id === selected)
  );
</script>
<div class="columns verification"><div class="stack">
  <Panel title="Verification state"><div class="row"><Badge tone={change.verification.tone}>{change.verification.label === 'Needs attention' ? 'Not verified' : change.verification.label}</Badge><strong>{change.satisfied} of {change.total} required Claims satisfied</strong></div><p>{change.blockers.map(blocker => blocker.text).join(' · ') || change.nextAction}</p></Panel>
  <Panel title="Claims"><div class="claim-list">{#each change.claims as claim}<section id={claim.id}><div class="row between claim-heading"><span class="mono">{claim.id} · Required</span><h2>{claim.title}</h2><Badge tone={claim.resolution === 'satisfied' ? 'ok' : claim.resolution === 'pending' ? 'warn' : 'danger'}>{claim.resolution.replaceAll('_',' ')}</Badge></div>
    {#each claim.checks as check}<div class="check" id={`check-${check.id}`}><div><h3>{check.name}</h3><p class="mono"><a href={`/t/${base.split('/')[2]}/cards/card_verifier_01`}>{check.verifier} v{check.version}</a> · {check.required ? 'required' : 'advisory'} · {check.mode === 'manual' ? 'manual' : 'on new evidence'}{check.billable ? ' · billable' : ''}</p></div><div><Badge tone={check.summary.tone}>{check.summary.label}</Badge>{#if check.provenance !== 'current'} <Badge tone={check.provenance === 'stale' ? 'warn' : 'neutral'}>{check.provenance.replaceAll('_',' ')}</Badge>{/if}<p>{check.explanation}</p></div><div class="check-actions">{#if check.eligible && view.capabilities.run}<button class="control" type="button" onclick={() => selected = check.id}>{check.action}</button>{/if}
      <details id={`result-${check.id}`}><summary>{check.evidence.some(item => !item.present) ? 'View missing Evidence' : check.execution === 'queued' ? 'View queue position' : check.execution === 'running' ? 'View run' : check.verdict === 'failed' ? 'View findings' : 'View result'} →</summary><div class="inspection"><h3>{check.name}</h3><p>Execution: {check.execution.replaceAll('_',' ')}</p><p>Verdict: {check.verdict ?? 'No verdict — run not completed'}</p><p>Provenance: {check.provenance.replaceAll('_',' ')} · bound to {check.revision}</p><p>{check.explanation}</p><section id={`evidence-${check.id}`}><h3>Evidence inputs</h3>{#each check.evidence as evidence}<div id={evidence.id}><p><strong>{evidence.name}</strong> · {evidence.present ? '✓ Received' : '○ Missing Evidence'}</p><p>{evidence.detail}</p>{#if evidence.digest}<p class="mono">{evidence.digest}</p>{/if}</div>{/each}</section>{#if check.history.length}<h3>Previous attempts</h3>{#each check.history as attempt}<p class="mono">{attempt.revision} · {attempt.execution.replaceAll('_',' ')} · verdict: {attempt.verdict ?? 'none'}</p>{/each}{/if}</div></details>
    </div></div>{/each}
  </section>{/each}</div><p class="muted">Approval and override do not turn unresolved verification into a pass.</p></Panel>
</div><aside class="stack">
  {#if result?.problem}<div class="notice" role="alert">{result.problem.title}<p class="mono">{result.problem.code}</p></div>{/if}{#if result?.message}<p role="status">{result.message}</p>{/if}
  <Panel title="Run eligibility"><p>○ Not ready — follow the prerequisite.</p><p>● Ready, manual — confirm the run.</p><p>◴ Verifying / queued — inspect existing work.</p><p>✓ / ✕ Completed — rerun only when offered.</p><p>Stale is provenance; it does not change a verdict.</p></Panel>
  {#if chosen}<div class="raised"><Panel title={`Confirm ${chosen.action?.toLowerCase()} — ${chosen.name}`} variant="raised"><p>{chosen.verifier} v{chosen.version} against revision {change.revisionNumber}</p><p>{chosen.billable ? 'This run is billable.' : 'This run has no charge.'} Previous results stay on the record.</p>{#if chosen.billable}<Badge tone="warn">Billable per run</Badge>{/if}<form method="POST" action="?/run" class="stack divider" use:enhance={() => async ({ update }) => { await update(); selected = ''; }}><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="checkId" value={chosen.id} /><input type="hidden" name="requestKey" value={view.requestKey} /><input type="hidden" name="confirmed" value="true" /><button class="control primary">Confirm {chosen.action?.toLowerCase()}</button><button class="control" type="button" onclick={() => selected = ''}>Cancel</button></form></Panel></div>{/if}
</aside></div>
<style>
  .verification p {
    font-size: 12px;
    margin: 5px 0;
  }
  .claim-heading {
    gap: 8px;
    padding: 10px 0;
    border-top: 2px dashed var(--border);
  }
  .claim-heading h2 {
    font-size: 13px;
    flex: 1;
    margin: 0;
  }
  .claim-list > section:first-child .claim-heading {
    border-top: 0;
    padding-top: 0;
  }
  .check {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr) auto;
    align-items: start;
    gap: 10px;
    padding: 7px 0;
  }
  .check h3 {
    font-size: 12px;
    margin: 0;
  }
  .check .mono,
  .check p {
    font-size: 10px;
  }
  .check-actions {
    display: grid;
    gap: 8px;
    justify-items: start;
  }
  .check-actions summary {
    font: 10px var(--font-mono);
    color: var(--brand-strong);
  }
  .inspection {
    min-width: 210px;
    margin-top: 10px;
  }
  .check:has(details[open]) {
    grid-template-columns: minmax(0, 1fr);
  }
  @media (max-width: 650px) {
    .check {
      grid-template-columns: minmax(0, 1fr);
      gap: 4px;
    }
    .claim-heading {
      align-items: start;
    }
    .claim-heading h2 {
      flex-basis: 100%;
    }
  }
</style>
