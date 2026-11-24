<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const problem = $derived(form?.problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? null : form?.problem);
  const report = $derived(form && 'report' in form ? form.report : null);
  let running = $state(false);
  const guarded = reauth.guard('Running stored-secret maintenance', false);
</script>

<header class="pagehead">
  <div><h1>Stored secrets</h1><p>The deployment key that protects every workspace's secrets.</p></div>
  <div class="actions">
    <form
      method="POST"
      action="?/run"
      use:enhance={(input) => {
        running = true;
        return async (outcome) => {
          running = false;
          await (await guarded(input))?.(outcome);
        };
      }}
    >
      <button class="app-control primary lg" type="submit" disabled={running} aria-busy={running ? 'true' : undefined}>{running ? 'Running…' : 'Run maintenance'}</button>
    </form>
  </div>
</header>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">
  {#if report}
    {#if report.remaining > 0 || report.bounded}
      <div class="note warn" data-testid="report"><span class="g">!</span><div><b>More to move.</b> <span class="muted">{report.remaining} secrets still use a previous key{report.bounded ? ' and this pass stopped before checking every store' : ''}. Run maintenance again.</span></div></div>
    {:else}
      <div class="note ok" data-testid="report"><span class="g">✓</span><div><b>Every secret this pass saw uses the active key.</b></div></div>
    {/if}
  {/if}
</div>

{#if report}
  <section class="card" aria-labelledby="last-run">
    <header><h2 id="last-run">This run</h2></header>
    <div class="row"><div class="label"><b>Moved to the active key</b></div><div class="ctl mono">{report.changed}</div></div>
    <div class="row"><div class="label"><b>Already current</b></div><div class="ctl mono">{report.current}</div></div>
    <div class="row"><div class="label"><b>Remaining</b><span>Failed, changed during the run, or past the run's time budget.</span></div><div class="ctl mono">{report.remaining}</div></div>
  </section>
  {#if report.failures.length}
    <div class="tablewrap">
      <table>
        <thead><tr><th>Store</th><th>Owner</th><th>Reason</th></tr></thead>
        <tbody>
          {#each report.failures as failure, index (index)}
            <tr><td class="mono">{failure.store}</td><td class="mono">{failure.owner}</td><td>{failure.reason}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{:else}
  <section class="card">
    <div class="row"><div class="label"><b>Routine rewrap</b><span>After a new wrapping key is active, run maintenance to move every stored secret onto it. Each run reports what it moved and what remains.</span></div></div>
  </section>
{/if}
