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
  <div class="columns"><div class="stack"><Panel title={`${change.author} opened this Change`}>{#snippet head()}<time class="mono muted" datetime={change.created}>{change.created.slice(0, 10)}</time>{/snippet}<p class="intent">{change.intent}</p><div class="kv divider"><span class="k">impact</span><span>{change.impact}</span><span class="k">owners</span><span>{change.owners}</span></div></Panel>
  <section id="subjects"><Panel title="Subjects"><Table label="Exact revision subjects"><table><thead><tr><th>Subject</th><th>Repository</th><th>Base → candidate</th><th>Pull request</th></tr></thead><tbody>{#each change.subjects as subject}<tr><td><a href={`${base}/subjects/${subject.id}?revision=${change.revision}`}><strong>{subject.id}</strong></a></td><td>{subject.repository}</td><td>{subject.base || 'Missing base'} → {subject.candidate || 'Missing candidate'}</td><td>{#if subject.url}<a href={subject.url} target="_blank" rel="noreferrer">#{subject.pr} ↗</a>{/if}<span class="muted"> · {subject.relationship}</span></td></tr>{/each}</tbody></table></Table></Panel></section>
</div><aside class="stack">
  <Panel title="Revision"><div class="kv"><span class="k">current</span><strong>revision {change.revisionNumber} · immutable</strong><span class="k">created</span><time class="mono" datetime={change.created}>{change.created}</time><span class="k">subjects</span><span>{change.subjects.length} exact</span></div><div class="row prior"> {#each change.priorRevisions as revision}<a href={`?revision=${revision}`}>{revision.replace('rev_', 'revision ')} →</a>{/each}</div></Panel>
  <section id="decisions"><Panel title="Override"><div class="kv"><span class="k">Override</span><span><Badge tone={change.override === 'None' ? 'neutral' : 'warn'}>{change.override}</Badge></span></div><p>Override is human judgment — it does not verify.</p>
  {#if view.capabilities.override}<details class="divider"><summary>Authorize override</summary><form method="POST" action="?/review" class="stack divider" use:enhance><input type="hidden" name="csrf" value={csrf} /><input type="hidden" name="revision" value={change.revision} /><input type="hidden" name="requestKey" value={`${view.requestKey}:override`} /><input type="hidden" name="operation" value="override" /><label>Claim<select name="decision">{#each change.claims as claim}<option value={claim.id}>{claim.id} — {claim.title}</option>{/each}</select></label><label>Reason<textarea name="body" required maxlength="10000"></textarea></label><button class="control">Authorize override</button></form></details>{/if}
  {#if result?.problem}<p role="alert">{result.problem.title} · {result.problem.code}</p>{/if}{#if result?.message}<p role="status">{result.message}</p>{/if}
  </Panel></section>
</aside></div></div>

<style>
  .intent {
    margin-top: 0;
    font-size: 14px;
  }
  .prior {
    margin-top: 8px;
    font-size: 12px;
  }
</style>
