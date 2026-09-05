<script lang="ts">
  import Badge from '$lib/components/Badge.svelte';
  import type { ChangeView } from './types';
  let { view, base, active }: { view: ChangeView; base: string; active: string } = $props();
  let change = $derived(view.change);
  let suffix = $derived(`?revision=${encodeURIComponent(change.revision)}`);
</script>
<svelte:head><title>{change.title} · {active} · Wyrd</title></svelte:head>
<header class="stack change-header">
  <div class="row between"><h1>{change.title}</h1><div class="row">{#if view.capabilities.review}<a class="control primary" href={`${base}/review${suffix}#submit-review`}>Review changes ▾</a>{/if}{#if view.capabilities.write}<details><summary class="control" aria-label="Change actions">⋯</summary><div class="menu"><a href={`${base}/timeline${suffix}#close`}>Close Change Request</a></div></details>{/if}</div></div>
  <div class="mono muted">{change.id} · revision {change.revisionNumber} (immutable) · opened by {change.author} · {change.subjects.length} subjects in {change.repositories.length} repositories</div>
  <div class="row channels"><span>Lifecycle <Badge tone={change.lifecycle === 'open' ? 'running' : 'neutral'}>{change.lifecycle === 'open' ? '' : '○ '}{change.lifecycle}</Badge></span><span>Verification <Badge tone={change.verification.tone}>{change.verification.label === 'Needs attention' ? 'Not verified' : change.verification.label} · {change.satisfied}/{change.total}</Badge></span><span>Approval <Badge tone="running">{change.approval}</Badge></span><span>Override <Badge tone={change.override === 'None' ? 'neutral' : 'warn'}>{change.override}</Badge></span></div>
  <p class="mono"><strong>Next:</strong> {change.nextAction}</p>
</header>
<nav class="nav" aria-label="Change workspace">{#each [['Overview',''],['Verification','/verification'],['Review','/review'],['Timeline','/timeline'],['Subjects','']] as [label,path]}<a href={`${base}${path}${suffix}${label === 'Subjects' ? '#subjects' : ''}`} aria-current={active === label ? 'page' : undefined}>{label}</a>{/each}</nav>
<style>
  .change-header {
    gap: 10px;
  }
  .change-header h1,
  .change-header p {
    margin: 0;
  }
  .channels {
    font: 10px var(--font-mono);
  }
  .channels > span {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  details {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    z-index: 3;
    min-width: 200px;
    padding: 14px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 6px 6px 0 var(--shadow);
  }
</style>
