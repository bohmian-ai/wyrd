<script lang="ts">
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import StatusIcon from './StatusIcon.svelte';
  import { enhance } from '$app/forms';
  import type { ChangeView, ActionResult, Claim } from './types';
  import { checkState, claimState } from './status';
  let {
    view,
    csrf,
    result
  }: { view: ChangeView; csrf: string; result?: ActionResult | null } = $props();
  let selected = $state('');
  let change = $derived(view.change);
  let checks = $derived(change.claims.flatMap((claim) => claim.checks));
  let counts = $derived({
    passed: checks.filter((check) => check.verdict === 'passed').length,
    failed: checks.filter((check) => check.verdict === 'failed').length,
    pending: checks.filter((check) => !check.verdict).length
  });
  let verified = $derived(change.verification.tone === 'ok');
  function passedNote(claim: Claim): string {
    const passed = claim.checks.filter((check) => check.verdict === 'passed');
    const revision = passed[0]?.revision.replace(/^rev_0?/, '') ?? change.revisionNumber;
    return `${passed.length} Verifier${passed.length === 1 ? '' : 's'} passed on revision ${revision}`;
  }
</script>
<div class="stack verification">
  {#if result?.problem}<div class="notice" role="alert">{result.problem.title}<p class="mono">{result.problem.code}</p></div>{/if}{#if result?.message}<p role="status">{result.message}</p>{/if}
  <div class="banner" class:ok={verified}>
    <Panel variant="accent">
      <div class="mb-row">
        <StatusIcon state={verified ? 'pass' : 'fail'} size={24} />
        <div class="mb-text">
          <h2>{change.verification.label === 'Needs attention' ? 'Not verified' : change.verification.label}</h2>
          <p class="mono counts">{change.satisfied} of {change.total} Claims satisfied · {counts.passed} passed · {counts.failed} failed · {counts.pending} pending</p>
          <p>{change.blockers.map(blocker => blocker.text).join('; ') || change.nextAction}</p>
        </div>
      </div>
    </Panel>
  </div>
  <Panel title="Claims">{#snippet head()}<span class="mono muted">{change.total} required</span>{/snippet}
    <div class="claim-list">{#each change.claims as claim}<details class="claim" id={claim.id} open={claim.resolution === 'pending'}>
    <summary class="claim-heading"><StatusIcon state={claimState(claim)} /><span class="claim-title"><span class="mono muted">{claim.id} · Required</span><strong>{claim.title}</strong>{#if claim.resolution === 'not_satisfied'}<span class="closed-lines">{#each claim.checks as check}<span class="closed-line" class:error={check.verdict === 'failed'}><StatusIcon state={checkState(check)} size={12} /> {check.summary.label} — {check.name}</span>{/each}</span>{/if}</span>{#if claim.resolution === 'satisfied'}<span class="closed-note mono">{passedNote(claim)}</span>{/if}</summary>
    {#each claim.checks as check}<div class="check" class:failed={check.verdict === 'failed'} id={`check-${check.id}`}>
      <StatusIcon state={checkState(check)} />
      <div class="check-name"><h3>{check.name}</h3><p class="mono muted">{check.verifier} v{check.version} · {check.required ? 'required' : 'advisory'} · {check.mode === 'manual' ? 'runs manually' : 'runs on new evidence'}{check.billable ? ' · billable' : ''}</p><p class="explanation">{check.explanation}</p></div>
      <div class="check-actions"><span class="badges" class:live={check.summary.tone === 'running'}><Badge tone={check.summary.tone}>{check.summary.label}</Badge>{#if check.provenance !== 'current'}<Badge tone={check.provenance === 'stale' ? 'warn' : 'neutral'}>{check.provenance.replaceAll('_',' ')}</Badge>{/if}</span>
      {#if check.eligible && view.capabilities.run}<button class="control secondary" type="button" onclick={() => selected = selected === check.id ? '' : check.id}>{check.action}</button>{/if}
      <details id={`result-${check.id}`}><summary>{check.evidence.some(item => !item.present) ? 'View missing Evidence' : 'Details'} →</summary><div class="inspection"><div class="kv"><span class="k">execution</span><span>{check.execution.replaceAll('_',' ')}</span><span class="k">verdict</span><span>{check.verdict ?? 'no verdict — run not completed'}</span><span class="k">provenance</span><span>{check.provenance.replaceAll('_',' ')} · bound to {check.revision}</span></div><section id={`evidence-${check.id}`}><h3>Evidence inputs</h3>{#each check.evidence as evidence}<div id={evidence.id}><p><strong>{evidence.name}</strong> · {evidence.present ? '✓ Received' : '○ Missing Evidence'}</p><p>{evidence.detail}</p>{#if !evidence.present}<p class="mono cli-hint">wyrd evidence submit --change {change.id} --check {check.id}</p>{/if}{#if evidence.digest}<p class="mono">{evidence.digest}</p>{/if}</div>{/each}</section>{#if check.history.length}<h3>Previous attempts</h3>{#each check.history as attempt}<p class="mono">{attempt.revision} · {attempt.execution.replaceAll('_',' ')} · verdict: {attempt.verdict ?? 'none'}</p>{/each}{/if}</div></details></div>
      {#if selected === check.id}<div class="confirm"><Panel title={`Confirm ${check.action?.toLowerCase()} — ${check.name}`} variant="raised"><p>{check.verifier} v{check.version} against revision {change.revisionNumber}</p><p>{check.billable ? 'This run is billable.' : 'This run has no charge.'} Previous results stay on the record.</p>{#if check.billable}<Badge tone="warn">Billable per run</Badge>{/if}<form method="POST" action="?/run" class="row divider" use:enhance={() => async ({ update }) => { await update(); selected = ''; }}><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="checkId" value={check.id} /><input type="hidden" name="requestKey" value={view.requestKey} /><input type="hidden" name="confirmed" value="true" /><button class="control primary">Confirm {check.action?.toLowerCase()}</button><button class="control" type="button" onclick={() => selected = ''}>Cancel</button></form></Panel></div>{/if}
    </div>{/each}
  </details>{/each}</div>
  {#if view.capabilities.write}<details class="amend"><summary>Amend Claims — creates revision {change.revisionNumber + 1}</summary>
    <form method="POST" action="?/revise" class="stack divider" use:enhance>
      <input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:revise`} />
      <div class="remove-list">{#each change.claims as claim}<label class="row"><input type="checkbox" name="remove" value={claim.id} /><span>Remove {claim.id} — {claim.title}</span></label>{/each}</div>
      <div class="fields"><label>New Claim title<input name="addTitle" maxlength="300" /></label>
      <label>Verifier<select name="addVerifier"><option value="">Choose later</option>{#each view.verifiers as verifier}<option value={verifier.name}>{verifier.name}{verifier.billable ? ' · billable' : ''}</option>{/each}</select></label></div>
      <label>Reason<textarea name="reason" required maxlength="10000"></textarea></label>
      <button class="control secondary">Create revision {change.revisionNumber + 1}</button>
      <small class="muted">Amending Claims creates an immutable new revision — approvals reset and reviewers re-review. CLI: <span class="mono">wyrd change revise</span>.</small>
    </form></details>
  {:else}<p class="muted amend">Claims are fixed for this revision. Amending them creates a new revision — <span class="mono">wyrd change revise</span>.</p>{/if}
  </Panel>
</div>
<style>
  .verification p {
    font-size: 12px;
    margin: 4px 0;
  }
  .mb-row {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  /* State as atmosphere: the verdict tints the whole banner body, not just a 24px icon. */
  .banner :global(.wy-panel-body) {
    background: color-mix(in srgb, var(--danger-text) 9%, var(--surface));
  }
  .banner.ok :global(.wy-panel-body) {
    background: color-mix(in srgb, var(--ok-text) 9%, var(--surface));
  }
  .check.failed {
    background: color-mix(in srgb, var(--danger-text) 6%, transparent);
    border-radius: var(--r);
  }
  /* A running Verifier breathes; frozen for viewers who ask for less motion. */
  .badges.live :global(.wy-badge) {
    animation: wy-breathe 2.4s ease-in-out infinite;
  }
  @keyframes wy-breathe {
    50% {
      opacity: 0.55;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .badges.live :global(.wy-badge) {
      animation: none;
    }
  }
  .mb-text {
    flex: 1;
    min-width: 0;
  }
  .mb-text h2 {
    margin: 0 0 4px;
    font-size: 19px;
  }
  .mb-text p {
    font-size: 13px;
    color: var(--muted);
  }
  .mb-text .counts {
    font-size: 12px;
    color: var(--text);
  }
  .claim summary.claim-heading {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 0;
    border-top: 1px solid var(--sep);
    cursor: pointer;
  }
  .cli-hint {
    color: var(--brand-strong);
  }
  .amend {
    margin: 14px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--sep);
    font-size: 12px;
  }
  details.amend summary {
    font: 700 11px var(--font-mono);
    color: var(--brand-strong);
    cursor: pointer;
  }
  .remove-list {
    display: grid;
    gap: 6px;
  }
  .remove-list label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .claim-list > .claim:first-child > .claim-heading {
    border-top: 0;
    padding-top: 2px;
  }
  .closed-lines {
    display: grid;
    gap: 4px;
    padding-top: 6px;
    font: 11px var(--font-mono);
    color: var(--muted);
  }
  .closed-line {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .closed-line.error {
    color: var(--danger-text);
    font-weight: 700;
  }
  .claim[open] > summary .closed-lines {
    display: none;
  }
  .claim-title {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .claim-title .mono {
    font-size: 10px;
  }
  .claim-title strong {
    font-size: 13px;
  }
  .closed-note {
    font-size: 10px;
    color: var(--muted);
    text-align: right;
    max-width: 46%;
    margin-left: auto;
  }
  .claim[open] > summary .closed-note {
    display: none;
  }
  .check {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) auto;
    align-items: start;
    gap: 6px 12px;
    padding: 12px 0 12px 28px;
  }
  .check h3 {
    font-size: 13px;
    margin: 0;
  }
  .check .mono,
  .check p {
    font-size: 11px;
    margin: 3px 0;
  }
  .check .explanation {
    font-size: 12px;
    font-family: var(--font-body, inherit);
    color: var(--muted);
  }
  .check-actions {
    display: grid;
    gap: 6px;
    justify-items: end;
  }
  .check-actions .badges {
    display: inline-flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .check-actions summary {
    font: 10px var(--font-mono);
    color: var(--brand-strong);
    cursor: pointer;
  }
  .inspection {
    min-width: 210px;
    margin-top: 10px;
    text-align: left;
  }
  .inspection .kv {
    margin: 6px 0;
  }
  .check:has(> .check-actions details[open]) {
    grid-template-columns: 16px minmax(0, 1fr);
  }
  .check:has(> .check-actions details[open]) .check-actions {
    grid-column: 2;
    justify-items: start;
  }
  .confirm {
    grid-column: 2 / -1;
  }
  @media (max-width: 650px) {
    .mb-row {
      flex-wrap: wrap;
    }
    .check {
      grid-template-columns: 16px minmax(0, 1fr);
      padding-left: 0;
    }
    .check-actions {
      grid-column: 2;
      justify-items: start;
    }
    .check-actions .badges {
      justify-content: flex-start;
    }
    .claim summary.claim-heading {
      flex-wrap: wrap;
    }
    .closed-note {
      max-width: 100%;
      text-align: left;
    }
  }
</style>
