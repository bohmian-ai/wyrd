<script lang="ts">
  import { MediaQuery } from 'svelte/reactivity';
  const narrow = new MediaQuery('(max-width: 600px)');
  let expanded = $state(true);
  $effect(() => {
    expanded = !narrow.current;
  });
  import Badge from '$lib/components/Badge.svelte';
  import type { ChangeView } from './types';
  let { view, base, active }: { view: ChangeView; base: string; active: string } = $props();
  let change = $derived(view.change);
  let suffix = $derived(`?revision=${encodeURIComponent(change.revision)}`);
</script>
<svelte:head><title>{change.title} · {active} · Wyrd</title></svelte:head>
<div class="compact-change-header">
 <h1 title={change.title}>{change.title}</h1>
 <p class="mono muted">{change.id} · revision {change.revisionNumber} · {change.subjects.length} subjects</p>
 <div class="compact-states"><Badge tone={change.lifecycle === 'open' ? 'running' : 'neutral'}>{change.lifecycle}</Badge><Badge tone={change.verification.tone}>{change.verification.label === 'Needs attention' ? 'Not verified' : change.verification.label} · {change.satisfied}/{change.total}</Badge><Badge tone="running">{change.approval.replace(' of ', '/')}</Badge></div>
 <p class="mono compact-next" title={change.nextAction}>Next: {change.nextAction}</p>
</div>
<details class="change-details" bind:open={expanded}>
 <summary class="compact-details">Revision details · navigation · Override: {change.override === 'None' ? 'none' : 'authorized, not verified'}</summary>
<header class="stack change-header">
  <div class="row between"><h1>{change.title}</h1><div class="row">{#if view.capabilities.review}<a class="control primary" href={`${base}${suffix}#submit-review`}>Review changes</a>{/if}{#if view.capabilities.write}<details><summary class="control" aria-label="Change actions">⋯</summary><div class="menu"><a href={`${base}${suffix}#close`}>Close Change Request</a></div></details>{/if}</div></div>
  <div class="row meta"><Badge tone={change.lifecycle === 'open' ? 'running' : 'neutral'}>{change.lifecycle === 'open' ? '' : '○ '}{change.lifecycle}</Badge><span class="mono muted">{change.id} · revision {change.revisionNumber} · opened by {change.author}</span></div>

</header>
<nav class="nav" aria-label="Change workspace">{#each [['Overview',''],['Verification','/verification'],['Audit','/audit'],['Files changed', change.subjects.length ? `/subjects/${change.subjects[0].id}` : '']] as [label,path]}<a href={`${base}${path}${suffix}${label === 'Files changed' && !path ? '#subjects' : ''}`} aria-current={active === label ? 'page' : undefined}>{label}</a>{/each}</nav>
</details>
<style>
  .compact-change-header,
  .compact-details {
    display: none;
  }
  @media (max-width: 600px) {
    .compact-change-header,
    .compact-details {
      display: block;
    }
    .compact-change-header h1 {
      margin: 0;
      font-size: 14px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .compact-change-header p {
      margin: 3px 0;
      line-height: 1.3;
    }
    .compact-states {
      display: flex;
      gap: 4px;
      flex-wrap: wrap;
    }
    .compact-states :global(.wy-badge) {
      padding: 2px 4px;
      font-size: 8.5px;
      letter-spacing: 0;
    }
    .compact-next {
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .compact-details {
      font: 9px var(--font-mono);
      padding: 3px 0 7px;
    }
    .change-details[open] {
      padding-bottom: 8px;
    }
  }

  .change-header {
    gap: 5px;
  }
  .change-header .meta {
    gap: 8px;
  }
  .change-header h1 {
    margin: 0;
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
