<!--
  The Repositories index: one row per Repository Card family or reachable
  GitHub repository without a Card. Registration, GitHub access and automatic
  verification are separate columns; the search and Show choice are intents.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { ACCESS_WORDS, READINESS_WORDS, readinessWhy, type RepositoryIndexProps } from '../repositories.js';
  import { REPOSITORY_SHOWS, type RepositoryIndexInput } from '../schemas.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  let { input, result, actions = {}, onaction }: RepositoryIndexProps & ActionProps = $props();

  const SHOW = { all: 'All', ready: 'Ready', inactive: 'Not verifying', undeclared: 'No Card' } as const;
  const show = $derived(input.show ?? 'all');
  const disabled = $derived(!onaction);
  const filter = (next: RepositoryIndexInput) =>
    onaction?.({ name: 'repository.filter', input: { ...(next.query ? { query: next.query } : {}), ...(next.show && next.show !== 'all' ? { show: next.show } : {}) } });
</script>

<RepositorySheet>
  <div class="page-head">
    <div class="grow">
      <h1>Repositories</h1>
      <p class="desc">Repository Cards and the GitHub repositories this tenant can reach. Registration, GitHub access and automatic verification are separate facts.</p>
    </div>
    <button type="button" class="btn btn-sm" {disabled} onclick={() => onaction?.({ name: 'connection.open', input: {} })}>GitHub connection</button>
  </div>
  {#if result.state === 'populated'}
    {@const data = result.data}
    {#if data.connection.state !== 'populated'}
      <p class="note box">GitHub connection unavailable: {data.connection.problem.title} ({data.connection.problem.code}). Access reads as each Repository Card reports it.</p>
    {:else if data.connection.data.state !== 'active'}
      <div class="banner warn" role="status">
        {data.connection.data.state === 'suspended' ? 'GitHub installation access is lost.' : 'GitHub is not connected.'} Registered Repository Cards stay readable; no new verification starts.
        <button type="button" class="link" {disabled} onclick={() => onaction?.({ name: 'connection.open', input: {} })}>Connection settings</button>
      </div>
    {/if}
    <div class="toolbar">
      <form
        class="toolbar"
        role="search"
        onsubmit={(event) => {
          event.preventDefault();
          filter({ query: String(new FormData(event.currentTarget).get('query') ?? '').trim(), show });
        }}
      >
        <input type="search" class="search" name="query" placeholder="Filter by Card or GitHub repository" aria-label="Filter repositories" value={input.query ?? ''} {disabled} />
      </form>
      <div class="segs" role="group" aria-label="Show">
        {#each REPOSITORY_SHOWS as key (key)}
          <button type="button" class="seg" aria-pressed={show === key} {disabled} onclick={() => filter({ query: input.query, show: key })}>{SHOW[key]}</button>
        {/each}
      </div>
      <span class="sub" aria-live="polite">{data.rows.length} of {data.total} shown</span>
      <ActionNote state={actions['repository.filter'] ?? actions['card.open'] ?? actions['repository.declare'] ?? actions['connection.open']} />
    </div>
    {#if data.partial}<p class="note box">{data.partial}</p>{/if}
    <div class="outs-wrap" role="region" aria-label="Repositories" tabindex="-1">
      <table class="outs">
        <thead>
          <tr><th scope="col">Repository</th><th scope="col">Card version</th><th scope="col">GitHub access</th><th scope="col">Automatic verification</th></tr>
        </thead>
        <tbody>
          {#each data.rows as row (row.card?.name ?? `github:${row.repositoryId}`)}
            <tr>
              <td>
                <div class="cellbox">
                  {#if row.card}
                    {@const card = row.card}
                    <button type="button" class="link mono" {disabled} onclick={() => onaction?.({ name: 'card.open', input: { card_ref: { kind: 'Repository', name: card.name, version: card.opens } } })}>{card.name}</button>
                    <span class="sub">Repository Card{row.fullName ? ` · ${row.fullName}` : ''}</span>
                  {:else}
                    <button type="button" class="link mono" {disabled} onclick={() => onaction?.({ name: 'repository.declare', input: { repository_id: row.repositoryId } })}>{row.fullName}</button>
                    <span class="sub"><span class="tag">no Card</span> GitHub repository only</span>
                  {/if}
                </div>
              </td>
              {#if row.problem}
                <td colspan="3"><div class="cellbox"><span class="muted">{row.problem.title} ({row.problem.code})</span></div></td>
              {:else}
                <td>
                  <div class="cellbox">
                    {#if row.card?.selected}
                      <span class="mono">{row.card.selected}</span><span class="sub">selected of {row.card.versions}</span>
                    {:else if row.card}
                      <span class="muted mono">none selected</span><span class="sub">{row.card.versions} version{row.card.versions === 1 ? '' : 's'}</span>
                    {:else}
                      <span class="muted mono">—</span>
                    {/if}
                  </div>
                </td>
                <td>
                  <div class="cellbox">
                    {#if row.access}
                      {@const fact = ACCESS_WORDS[row.access]}
                      <span class="st c-{fact.tone}"><span aria-hidden="true">{fact.glyph}</span>{fact.word}</span>
                    {/if}
                  </div>
                </td>
                <td>
                  <div class="cellbox">
                    {#if row.readiness}
                      {@const fact = READINESS_WORDS[row.readiness]}
                      <span class="st c-{fact.tone}"><span aria-hidden="true">{fact.glyph}</span>{fact.word}</span>
                      <span class="sub">{readinessWhy(row.readiness, row.bindings, row.card?.selected)}</span>
                    {/if}
                  </div>
                </td>
              {/if}
            </tr>
          {:else}
            <tr>
              <td colspan="4"><div class="cellbox"><span class="prose muted">{data.total === 0 ? 'No Repository Card is registered and the GitHub connection reaches no repository.' : 'No repositories match this filter.'}</span></div></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="note">Granting GitHub access never registers a Repository Card or binds Verifiers. Declare a wyrd.toml to register one.</p>
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading repositories" />
  {:else if result.state !== 'empty'}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</RepositorySheet>
