<script lang="ts">
  import '$lib/features/cards/cards.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import SpecSections from '$lib/features/cards/SpecSections.svelte';
  import { workspaces } from '$lib/features/cards/core/workspace-registry';
  import { page } from '$app/state';
  let { data } = $props();
  const detail = $derived(data.detail);
  const base = $derived(page.url.pathname.replace(/\/cards\/[^/]+$/, ''));
  const workspace = $derived(workspaces.get(detail.kind));
  // A page-layout workspace projects its own operational assessment, so the
  // shared header drops its status chip — one status authority per page.
  const pageWorkspace = $derived(workspace?.layout === 'page' && !!detail.presentation);
  const viewingPrior = $derived(detail.version !== detail.currentVersion);
  function versionHref(version: string): string {
    return version === detail.currentVersion ? page.url.pathname : `?version=${version}`;
  }
</script>

<svelte:head><title>{detail.name} · Cards · Wyrd</title></svelte:head>
<div class="cards">
  <header class="card-head">
    <div class="row">
      <h1>{detail.name}</h1>
      <span class="kind-pill">● {detail.kind}</span>
      <span class="mono muted"
        >{detail.version}{detail.version === detail.currentVersion ? ' (current)' : ''}</span
      >
      {#if !pageWorkspace}
        <Badge tone={detail.status.tone}>{detail.status.label}</Badge>
      {/if}
      <span class="row" style="margin-left:auto">
        {#each detail.actions as action (action.label)}
          <a class="control" href={base + action.href}>{action.label}</a>
        {/each}
      </span>
    </div>
    <p class="mono muted">
      {detail.uid} · apiVersion wyrd/v1 · read-only projection of server truth
    </p>
    <p>{detail.summary}</p>
    {#if viewingPrior}
      <div class="notice">
        <p class="mono muted">
          Viewing {detail.version} read-only — {detail.currentVersion} is current.
          <a href={page.url.pathname}>Back to {detail.currentVersion}</a>
        </p>
      </div>
    {/if}
  </header>
  {#if workspace?.layout === 'page' && detail.presentation}
    <!-- A page-layout workspace owns everything below the shared header —
         its views carry their own rail, versions and local navigation. -->
    {@const Presentation = workspace.component}
    <Presentation {detail} {base} />
  {:else}
  <div class="columns">
    <div class="stack">
      {#if workspace && !workspace.layout && detail.presentation}
        {@const Presentation = workspace.component}
        <Presentation {detail} {base} />
      {:else}
        <SpecSections sections={detail.spec} {base} />
      {/if}
      <Panel title="Versions" variant="raised">
        {#snippet head()}<span class="mono muted">immutable · newest first</span>{/snippet}
        <Table label="Versions">
          <table>
            <thead><tr><th>Version</th><th>Created</th><th>By</th><th>Note</th></tr></thead>
            <tbody>
              {#each detail.versions as row (row.version)}
                <tr class={row.version === detail.version ? 'sel' : ''}>
                  <td class="mono"
                    ><a href={versionHref(row.version)}
                      ><strong
                        >{row.version}{row.version === detail.currentVersion
                          ? ' (current)'
                          : ''}</strong
                      ></a
                    ></td
                  >
                  <td class="mono">{row.created}</td>
                  <td class="mono">{row.by}</td>
                  <td class="mono">{row.note}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </Table>
      </Panel>
    </div>
    <div class="stack">
      <Panel title="Metadata" variant="flat">
        {#snippet head()}<span class="mono muted">server-managed</span>{/snippet}
        <div class="kv">
          {#each detail.metadata as entry (entry.k)}
            <span class="k">{entry.k}</span><span class="mono">{entry.v}</span>
          {/each}
        </div>
      </Panel>
      <Panel title="Relationships" variant="flat">
        {#snippet head()}<span class="mono muted">server-derived</span>{/snippet}
        {#if !detail.relationships.length}
          <StateBlock
            state="absent"
            title="No relationships"
            detail="The server has derived no relationships for this Card version."
          />
        {:else}
          <div class="kv">
            {#each detail.relationships as rel (rel.relation + rel.label)}
              <span class="k">{rel.relation}</span>
              {#if rel.href}<span class="mono"><a href={base + rel.href}>{rel.label}</a></span>
              {:else}<span class="mono">{rel.label}</span>{/if}
            {/each}
          </div>
        {/if}
      </Panel>
    </div>
  </div>
  {/if}
</div>
