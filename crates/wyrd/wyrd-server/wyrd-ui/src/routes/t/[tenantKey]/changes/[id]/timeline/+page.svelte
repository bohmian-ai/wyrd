<script lang="ts">
  import '$lib/features/changes/changes.css';
  import { enhance } from '$app/forms';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import Table from '$lib/components/Table.svelte';
  let { data, form } = $props();
  let base = $derived(`/t/${data.tenant.key}/changes/${data.change.id}`);
</script>
<div class="changes"><ChangeHeader view={data} {base} active="Timeline" /><div class="columns"><div class="stack">
<form method="GET" class="row"><input type="hidden" name="revision" value={data.change.revision} /><label>Event kind<select name="kind" value={data.kind}><option value="">All</option>{#each ['revision','commit','pr','evidence','run','result','claim','decision','discussion','lifecycle'] as kind}<option value={kind}>{kind}</option>{/each}</select></label><button class="control">Filter</button></form>
<Table label="Change chronology"><table><thead><tr><th>Time / source</th><th>Type / actor</th><th>What happened</th><th>Destination</th></tr></thead><tbody>{#each data.change.timeline.filter(event => !data.kind || event.kind === data.kind) as event}<tr><td><time datetime={event.at}>{event.at.replace('2026-','').replace('T',' ')}</time><p>{event.source}</p></td><td>{event.kind}<p>{event.actor}</p></td><td class="event-text">{event.text}</td><td><a href={base + event.destination}>Open {event.kind} →</a></td></tr>{/each}</tbody></table></Table>
</div><aside class="stack"><Panel title="Audit and discussion"><p>Audit — server-derived, immutable events.</p><p>Review activity — human discussion with immutable edit history and reversible resolution.</p></Panel>
{#if data.capabilities.write && data.change.lifecycle !== 'closed'}<section id="close"><Panel title="Close Change Request"><form method="POST" action="?/review" class="stack" use:enhance><input type="hidden" name="csrf" value={data.session.csrf} /><input type="hidden" name="revision" value={data.change.revision} /><input type="hidden" name="requestKey" value={`${data.requestKey}:close`} /><input type="hidden" name="operation" value="close" /><label>Closure reason<select name="decision"><option value="completed">Completed</option><option value="cancelled">Cancelled</option></select></label><p>This records a closure reason. Provider pull requests stay unchanged.</p><button class="control">Close Change Request</button></form></Panel></section>{/if}
{#if form?.problem}<p class="notice" role="alert">{form.problem.title} · {form.problem.code}</p>{/if}{#if form?.message}<p role="status">{form.message}</p>{/if}
</aside></div></div>
<style>
  .event-text {
    white-space: normal !important;
    min-width: 220px;
    max-width: 420px;
  }
</style>
