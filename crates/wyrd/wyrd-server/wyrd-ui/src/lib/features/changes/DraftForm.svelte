<script lang="ts">
  import { untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  const narrow = new MediaQuery('(max-width: 600px)');
  import { enhance } from '$app/forms';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import type { Draft, ActionResult, VerifierChoice } from './types';
  let {
    initial,
    verifiers,
    csrf,
    requestKey,
    base,
    revision = '',
    result
  }: {
    initial: Draft;
    verifiers: VerifierChoice[];
    csrf: string;
    requestKey: string;
    base: string;
    revision?: string;
    result?: ActionResult | null;
  } = $props();
  let draft = $state<Draft>(untrack(() => structuredClone(initial)));
  let busy = $state(false);
  $effect(() => {
    if (result?.draft) draft = structuredClone(result.draft);
  });
  function addSubject() {
    draft.subjects.push({
      id: crypto.randomUUID(),
      repository: '',
      provider: 'github',
      pr: '',
      url: '',
      base: '',
      candidate: '',
      relationship: '',
      commits: [],
      files: []
    });
  }
  function addClaim() {
    draft.claims.push({
      id: crypto.randomUUID(),
      title: '',
      checks: [{ name: '', required: true, mode: 'manual', billable: true }]
    });
  }
</script>
<form method="POST" action="?/save" class="stack draft" use:enhance={() => { busy = true; return async ({ update }) => { await update({ reset: false }); busy = false; }; }}>
  <input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="requestKey" value={requestKey} /><input type="hidden" name="revision" value={revision} /><input type="hidden" name="subjects" value={JSON.stringify(draft.subjects)} /><input type="hidden" name="claims" value={JSON.stringify(draft.claims)} />
  <div class="row between"><div><h1>{revision ? 'Edit draft' : 'New Change Request'}</h1><p class="muted">Save an incomplete draft at any point.</p></div><div class="row"><button class="control primary" disabled={busy}>Save draft</button><a href={base}>Discard</a></div></div>
  {#if result?.problem}<StateBlock state="error" title={result.problem.title} code={result.problem.code} detail="Your draft is preserved. Check the fields and retry." />{/if}
  <div class="columns"><div class="stack">
    <Panel title="1 · What and why"><div class="stack"><label>Title<input name="title" bind:value={draft.title} maxlength="300" /></label><details open={!narrow.current}><summary>Intent and impact</summary><div class="stack"><label>Intent<textarea name="intent" bind:value={draft.intent} rows="2"></textarea></label><label>Impact<input name="impact" bind:value={draft.impact} /></label></div></details></div></Panel>
    <Panel title="2 · Subjects"><div class="stack"><div><label for="pr-url">Link a pull request — resolves to exact commits</label><div class="row"><input id="pr-url" class="pr-url" name="prUrl" type="url" placeholder="https://github.com/acme/ranking/pull/221" /><button class="control" formaction="?/resolve" disabled={busy}>Resolve</button></div></div>
      {#each draft.subjects as subject, i (subject.id)}<div class="subject-row"><div class="row between mono"><strong>{subject.repository || 'New subject'}</strong><span>{subject.provider} · {subject.pr ? `#${subject.pr}` : 'No PR'}</span><span>{subject.base || '—'} → {subject.candidate || '—'}</span><span>{subject.candidate ? '✓ Resolved' : '✕ Missing candidate'}</span></div><div class="row"><details open={!subject.repository}><summary>{subject.candidate ? 'Edit' : 'Repair'}</summary><div class="fields divider"><label>Repository<input bind:value={subject.repository} /></label><label>Provider<input bind:value={subject.provider} /></label><label>Base commit<input bind:value={subject.base} /></label><label>Candidate commit<input bind:value={subject.candidate} /></label><label>Pull request<input bind:value={subject.pr} /></label><label>Provider URL<input type="url" bind:value={subject.url} /></label></div></details><button type="button" class="text-action" onclick={() => draft.subjects.splice(i,1)}>Remove</button></div>{#if !subject.candidate}<p class="error">No candidate commit — repair this subject. The draft still saves.</p>{/if}</div>{/each}
      <button class="control" type="button" onclick={addSubject}>+ Add by exact commits</button></div></Panel>
    <Panel title="3 · Claims and Verifiers"><details open={!narrow.current}><summary>{draft.claims.length} Claims · edit requirements</summary><div class="stack">{#each draft.claims as claim, i (claim.id)}<div class="claim-row"><label class="claim-name">Claim<input bind:value={claim.title} /></label>{#each claim.checks as check, j}<div class="check-fields"><label>Requirement<select bind:value={check.required}><option value={true}>Required</option><option value={false}>Advisory</option></select></label><label>Verifier<select bind:value={check.name}><option value="">Select Verifier</option>{#each verifiers as verifier}<option value={verifier.name}>{verifier.name} v{verifier.version}{verifier.billable ? " · billable" : ""}</option>{/each}</select></label><label>Mode<select bind:value={check.mode}><option value="manual">Manual</option><option value="on-new-evidence">On new evidence</option></select></label><button type="button" class="text-action" onclick={() => claim.checks.splice(j,1)}>Remove Verifier</button></div>{/each}<details><summary>Edit requirements</summary><div class="row"><button type="button" class="control" onclick={() => claim.checks.push({ name: '', required: true, mode: 'manual', billable: true })}>+ Add Verifier</button><button type="button" class="control" onclick={() => draft.claims.splice(i,1)}>Remove Claim</button></div></details></div>{/each}<button type="button" class="control" onclick={addClaim}>+ Add Claim</button></div></details></Panel>
  </div><aside class="stack"><Panel title="Progress"><p>○ Draft — all sections stay editable</p><p>{draft.subjects.filter(subject => !subject.base || !subject.candidate).length} subjects need an exact commit pair.</p></Panel><Panel title="Owners and teams"><div class="stack"><label>Owner<input name="owner" bind:value={draft.owner} /></label><label>Participating teams<textarea name="teams" bind:value={draft.teams} rows="3"></textarea></label></div></Panel><StateBlock state="absent" title="Evidence" detail="Evidence acceptance becomes available after a revision and an exact subject resolve." /></aside></div>
  <div class="footer row"><div class="cost-warning"><strong>! Billable mode</strong><p>On new evidence can start a paid run without further confirmation. Manual runs require confirmation.</p></div><button class="control primary" disabled={busy}>{busy ? 'Saving…' : 'Save draft'}</button><a href={base}>Discard</a></div>
</form>
<style>
  .draft .stack {
    gap: 8px;
  }
  .draft textarea {
    min-height: 36px;
  }
  .draft label {
    font-size: 10px;
    gap: 4px;
  }
  .draft :is(input, select, textarea) {
    padding: 5px;
    font-size: 11px;
  }
  .pr-url {
    flex: 1;
  }
  .subject-row {
    border-bottom: 2px dashed var(--border);
    padding: 6px 0;
  }
  .subject-row p {
    font-size: 11px;
    margin: 4px 0;
  }
  .subject-row details {
    font-size: 11px;
  }
  .subject-row details[open] {
    flex-basis: 100%;
  }
  .claim-row {
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1.75fr);
    gap: 8px;
    align-items: start;
  }
  .check-fields {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .claim-row details {
    grid-column: 1 / -1;
    font-size: 10px;
  }
  .check-fields .text-action {
    grid-column: 1 / -1;
    justify-self: start;
  }
  .text-action {
    border: 0;
    background: transparent;
    color: var(--brand-strong);
    font: 10px var(--font-mono);
    cursor: pointer;
    padding: 2px;
  }
  .cost-warning {
    flex: 1;
    font-size: 11px;
  }
  .cost-warning p {
    margin: 4px 0;
  }
  @media (max-width: 600px) {
    .claim-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .cost-warning {
      flex-basis: 100%;
    }
    .draft .footer {
      padding-bottom: 36px;
    }
  }
</style>
