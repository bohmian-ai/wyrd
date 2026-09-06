<script lang="ts">
  import '$lib/features/changes/changes.css';
  import Badge from '$lib/components/Badge.svelte';
  import Button from '$lib/components/Button.svelte';
  import Table from '$lib/components/Table.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import { page } from '$app/state';
  let { data } = $props();
  const views = [
    ['open', 'Open'],
    ['needs-attention', 'Needs attention'],
    ['verified', 'Verified'],
    ['closed', 'Closed']
  ];
  function viewLink(view: string) {
    const query = new URLSearchParams(page.url.search);
    query.set('view', view);
    return `?${query}`;
  }
</script>
<svelte:head><title>Change Requests · Wyrd</title></svelte:head>
<div class="changes">
  <div class="row between"><div><h1>Change Requests</h1><p class="muted">Work needing action first — see why and who is needed.</p></div><Button href={`${page.url.pathname}/new`}>+ New Change Request</Button></div>
  <nav class="nav" aria-label="Change views">{#each views as [view,label]}<a href={viewLink(view)} aria-current={data.filters.view === view ? 'page' : undefined}>{label}{#if data.list} · {data.list.counts[view]}{/if}</a>{/each}</nav>
  <form class="filters stack" method="GET"><input type="hidden" name="view" value={data.filters.view} /><div class="row"><input aria-label="Search Change Requests" name="q" value={data.filters.q} placeholder="Title, owner, service, repository, PR…" /><button class="control">Search</button><details><summary>+ Add filter</summary><div class="fields">{#each ['owner','team','repository','lifecycle'] as field}<label>{field}<input name={field} value={data.filters[field]} /></label>{/each}</div></details></div></form>
  {#if data.problem}<StateBlock state={data.problem.status === 403 ? 'unauthorized' : 'error'} title={data.problem.title} code={data.problem.code} actionLabel="Retry" actionHref={page.url.pathname + page.url.search} />
  {:else if !data.list?.records.length}<StateBlock state="empty" title="No matching Change Requests" detail="Your view and filters are preserved." actionLabel="Clear search and filters" actionHref={`?view=${data.filters.view}`} />
  {:else}<Table label="Change Requests"><table><thead><tr>{#each ['Change','Lifecycle','Verification','Claims','Repos / PRs','Activity'] as heading}<th>{heading}</th>{/each}</tr></thead><tbody>{#each data.list.records as change}<tr><td><a href={`${page.url.pathname}/${change.id}`}><strong class="table-title">{change.title}</strong></a><p class="mono muted">{change.id} · {change.owner}</p>{#each change.attention as attention}<p class="attention-line"><Badge tone="warn">{attention.reason}</Badge><span class="muted waiting">waiting on {attention.actor}</span></p>{/each}</td><td><Badge tone={change.lifecycle === 'open' ? 'running' : 'neutral'}>{change.lifecycle === 'open' ? '' : '○ '}{change.lifecycle}</Badge>{#if change.closure}<p>{change.closure}</p>{/if}</td><td><Badge tone={change.verification.tone}>{change.verification.label}</Badge></td><td>{change.satisfied}/{change.total} required</td><td>{#each change.repositories as repository}<div>{repository}</div>{/each}<p>{change.prs.map(pr => `#${pr}`).join(' · ')}</p></td><td>{change.activity}</td></tr>{/each}</tbody></table></Table>{/if}
</div>
<style>
  .attention-line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  td :global(.table-title) {
    margin: 0 0 2px;
  }
</style>
