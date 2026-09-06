<script lang="ts">
  import '$lib/features/changes/changes.css';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import Table from '$lib/components/Table.svelte';
  let { data } = $props();
  let base = $derived(`/t/${data.tenant.key}/changes/${data.change.id}`);
</script>
<div class="changes"><ChangeHeader view={data} {base} active="Audit" /><div class="columns"><div class="stack">
<nav class="nav kind-filter" aria-label="Event kind">{#each ['', 'revision','commit','pr','evidence','run','result','claim','decision','discussion','lifecycle'] as kind}<a href={`?revision=${encodeURIComponent(data.change.revision)}&kind=${kind}`} aria-current={(data.kind || '') === kind ? 'page' : undefined}>{kind || 'All'}</a>{/each}</nav>
<Table label="Change chronology"><table><thead><tr><th>Time</th><th>Type</th><th>Actor</th><th>What happened</th><th>Destination</th></tr></thead><tbody>{#each data.change.timeline.filter(event => !data.kind || event.kind === data.kind) as event}<tr><td class="mono"><time datetime={event.at}>{event.at.replace('2026-','').replace('T',' ').replace(/:\d\d(\.\d+)?Z$/,'Z')}</time></td><td><span class="kind mono">{event.kind}</span>{#if event.source !== 'Audit'}<span class="mono muted">&nbsp;· review</span>{/if}</td><td class="mono">{event.actor}</td><td class="event-text">{event.text}</td><td><a href={base + event.destination}>Open {event.kind} →</a></td></tr>{/each}</tbody></table></Table>
</div><aside class="stack"><Panel title="Audit and discussion"><p>Audit — server-derived, immutable events. This page is the record.</p><p>Review activity — human discussion; read and reply in <a href={`${base}?revision=${encodeURIComponent(data.change.revision)}`}>the Overview conversation</a>.</p></Panel>
</aside></div></div>
<style>
  .event-text {
    white-space: normal !important;
    min-width: 220px;
    max-width: 420px;
  }
  .kind {
    color: var(--brand-strong);
    font-weight: 700;
  }
  .kind-filter {
    margin-bottom: 12px;
  }
</style>
