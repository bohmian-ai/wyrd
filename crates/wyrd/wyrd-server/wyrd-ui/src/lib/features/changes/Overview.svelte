<script lang="ts">
  import { enhance } from '$app/forms';
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import Table from '$lib/components/Table.svelte';
  import type { ChangeView, ActionResult } from './types';
  let {
    view,
    base,
    csrf,
    result
  }: { view: ChangeView; base: string; csrf: string; result?: ActionResult | null } =
    $props();
  let change = $derived(view.change);
</script>
<div class="stack overview">
  {#if change.blockers.length}<Panel title={`Action summary — revision ${change.revisionNumber}`} variant="raised"><div class="stack action-summary">{#each change.blockers as blocker}<div><strong class="error">✕ {blocker.text}</strong><p class="mono">→ Waiting on {blocker.actor}</p></div>{/each}<p>Your available action: <a href={`${base}/review`}>Review changes — Comment · Approve · Request changes</a></p></div></Panel>{/if}
  <div class="columns"><div class="stack"><Panel title="Intent and impact"><p>{change.intent}</p><p><strong>Impact</strong> · {change.impact}</p><p><strong>Owners</strong> · {change.owners}</p></Panel>
  <section id="subjects"><h2>Subjects</h2><Table label="Exact revision subjects"><table><thead><tr><th>Subject</th><th>Repository</th><th>Base → candidate</th><th>Pull request</th></tr></thead><tbody>{#each change.subjects as subject}<tr><td><a href={`${base}/subjects/${subject.id}?revision=${change.revision}`}><strong>{subject.id}</strong></a></td><td>{subject.repository}</td><td>{subject.base || 'Missing base'} → {subject.candidate || 'Missing candidate'}</td><td>{#if subject.url}<a href={subject.url} target="_blank" rel="noreferrer">#{subject.pr} ↗</a>{/if}<span class="muted"> · {subject.relationship}</span></td></tr>{/each}</tbody></table></Table></section>
  <Panel title="Claims"><div class="stack">{#each change.claims as claim}<div class="row between claim-row"><span class="mono">{claim.id}</span><strong>{claim.title}</strong><Badge tone={claim.resolution === 'satisfied' ? 'ok' : claim.resolution === 'not_satisfied' ? 'danger' : 'warn'}>{claim.resolution === 'not_satisfied' ? 'Not satisfied' : claim.resolution}</Badge><a href={`${base}/verification?revision=${change.revision}#${claim.id}`}>Open →</a></div>{/each}</div></Panel>
</div><aside class="stack">
  <Panel title="Revision"><p>Current selection: revision {change.revisionNumber}</p><p class="mono"><time datetime={change.created}>{change.created}</time></p><p>{change.subjects.length} exact subjects · immutable</p><div class="row">{#each change.priorRevisions as revision}<a href={`?revision=${revision}`}>{revision.replace('rev_', 'revision ')} →</a>{/each}</div></Panel>
  <Panel title="Verification"><Badge tone={change.verification.tone}>{change.verification.label}</Badge><p>{change.satisfied} of {change.total} required Claims satisfied</p><a href={`${base}/verification?revision=${change.revision}`}>Open Verification →</a></Panel>
  <section id="decisions"><Panel title="Decisions"><p>Human judgment · separate from verification</p><Badge tone="running">{change.approval}</Badge><p>Review decisions and their authors are recorded in the Timeline.</p><Badge tone={change.override === 'None' ? 'neutral' : 'warn'}>Override: {change.override}</Badge><p>Approval and override do not verify.</p>
  {#if view.capabilities.override}<details class="divider"><summary>Authorize override</summary><form method="POST" action="?/review" class="stack divider" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:override`} /><input type="hidden" name="operation" value="override" /><label>Claim<select name="decision">{#each change.claims as claim}<option value={claim.id}>{claim.id} — {claim.title}</option>{/each}</select></label><label>Reason<textarea name="body" required maxlength="10000"></textarea></label><button class="control">Authorize override</button></form></details>{/if}
  {#if result?.problem}<p role="alert">{result.problem.title} · {result.problem.code}</p>{/if}{#if result?.message}<p role="status">{result.message}</p>{/if}
  </Panel></section>
</aside></div></div>

<style>
  .action-summary {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
    align-items: center;
  }
  .action-summary strong {
    font-size: 12px;
  }
  .overview .claim-row {
    gap: 6px;
    font-size: 12px;
  }
  .overview .claim-row > strong {
    flex: 1;
  }
  @media (max-width: 900px) {
    .action-summary {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .action-summary p {
    margin: 3px 0;
  }
</style>
