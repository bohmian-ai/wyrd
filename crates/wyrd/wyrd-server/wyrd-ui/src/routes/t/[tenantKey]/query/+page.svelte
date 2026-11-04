<script lang="ts">
  import { enhance } from '$app/forms';
  import { page } from '$app/state';
  import { EmptyState, RecordTable } from 'wyrd-ai-ui';
  import type { PageProps } from './$types';

  let { data, form }: PageProps = $props();
  const sql = $derived(page.url.searchParams.get('sql') ?? '');
  const params = $derived(page.url.searchParams.get('params') ?? '[]');
  const binds = $derived.by((): unknown[] => {
    try {
      const parsed: unknown = JSON.parse(params);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  });
  // Only a same-origin path returns; anything else goes back to Observe.
  const back = $derived(page.url.searchParams.get('back')?.startsWith('/t/') ? page.url.searchParams.get('back')! : `/t/${encodeURIComponent(data.tenant.key)}/observe`);
  let running = $state<AbortController | undefined>();
  let cancelled = $state(false);
</script>

<svelte:head><title>Query · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="app-page">
<h1>Query</h1>
<p><a href={back}>← Back to Observe</a></p>

{#if !sql}
  <EmptyState state="empty" title="Open a list or chart's query from Observe to inspect it here." />
{:else}
  <section aria-label="Executed query">
    <h2>Executed SQL</h2>
    <pre><code>{sql}</code></pre>
    <h2>Binds</h2>
    {#if binds.length}
      <table class="binds">
        <thead><tr><th scope="col">Bind</th><th scope="col">Type</th><th scope="col">Value</th></tr></thead>
        <tbody>
          {#each binds as value, index (index)}
            <tr><td>${index + 1}</td><td>{value === null ? 'null' : typeof value}</td><td><code>{JSON.stringify(value)}</code></td></tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p>No binds.</p>
    {/if}
  </section>

  <form
    method="POST"
    action="?/run"
    use:enhance={({ controller }) => {
      running = controller;
      cancelled = false;
      return async ({ update }) => {
        running = undefined;
        await update({ reset: false });
      };
    }}
  >
    <input type="hidden" name="sql" value={sql} />
    <input type="hidden" name="params" value={params} />
    <button class="app-control" type="submit" disabled={running !== undefined}>{running ? 'Running…' : 'Run again'}</button>
    {#if running}
      <button class="app-control" type="button" onclick={() => ((cancelled = true), running?.abort(), (running = undefined))}>Cancel</button>
    {/if}
  </form>

  {#if cancelled}
    <p role="status">Cancelled; no result was kept.</p>
  {:else if form?.problem}
    <EmptyState state={form.problem.status === 401 || form.problem.status === 403 ? 'denied' : 'error'} problem={form.problem} />
  {:else if form?.result}
    <p role="status">
      {form.result.terminal.outcome} · {form.result.terminal.row_count} rows{form.result.terminal.warnings.length ? ` · ${form.result.terminal.warnings.join(', ')}` : ''}
    </p>
    <RecordTable input={{ sql }} result={form.result.rows.length ? { state: 'populated', data: { rows: form.result } } : { state: 'empty', data: { rows: form.result }, reason: 'The SQL returned no rows.' }} />
  {/if}
{/if}
</div>

<style>
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
    margin-bottom: 8px;
  }
  h2 {
    font: var(--weight-strong) 14px var(--font-sans);
    margin: 16px 0 6px;
  }
  pre {
    overflow: auto;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    font: 12px var(--font-mono);
    white-space: pre-wrap;
  }
  .binds {
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  .binds th,
  .binds td {
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
    text-align: left;
  }
  form {
    display: flex;
    gap: 8px;
    margin: 16px 0;
  }
  a {
    color: var(--declare);
  }
</style>
