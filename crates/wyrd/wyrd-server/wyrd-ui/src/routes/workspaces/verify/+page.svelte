<!--
  The page a discovery link opens: the workspaces where the proven address
  is a verified recovery email, each linking only to its own sign-in page.
  Later pages read with the five-minute read cookie the proof set.
-->
<script lang="ts">
  import Proof from '$lib/Proof.svelte';
  import { PROBLEMS, PROOF_REFUSALS, auth, refusal } from '$lib/onboarding';
  import type { WyrdProblem } from '$lib/server/problem';

  type Workspace = { tenant: string; display_name: string; login_url: string };
  let shown: Workspace[] = $state([]);
  let cursor: string | null = $state(null);
  let loading = $state(false);
  let issue: WyrdProblem | null = $state.raw(null);

  /** Take one page of the server's answer. */
  function take(body: Record<string, unknown> | null) {
    shown = [...shown, ...((body?.workspaces as Workspace[] | undefined) ?? [])];
    cursor = typeof body?.next_cursor === 'string' ? body.next_cursor : null;
  }

  async function verify(proof: string) {
    const answer = await auth('/auth/browser/workspaces/verify', { proof });
    if (answer.status !== 200) return refusal(answer, PROOF_REFUSALS);
    take(answer.body);
    return null;
  }

  async function more() {
    loading = true;
    const answer = await auth(`/auth/browser/workspaces?cursor=${encodeURIComponent(cursor ?? '')}`);
    loading = false;
    if (answer.status === 200) take(answer.body);
    else issue = refusal(answer, { WYRD_AUTH_400_INVALID_STATE: PROBLEMS.readExpired });
  }
</script>

<svelte:head><title>Your workspaces · Wyrd</title></svelte:head>
<Proof heading="Find your workspaces" doneHeading="Your workspaces" {verify} restart={['/workspaces', 'Request a new link']}>
  {#snippet done()}
    {#if issue}
      <div class="app-problem" role="alert" data-state="unauthorized"><strong>⊘ {issue.title}</strong><p>{issue.remediation}</p></div>
    {/if}
    {#if shown.length}
      <p class="center">Sign in to each workspace on its own page. This list stays readable for five minutes.</p>
      <ul aria-label="Workspaces for this address">
        {#each shown as workspace (workspace.tenant)}
          <li><a href={workspace.login_url}><span class="name">{workspace.display_name}</span><code>{workspace.tenant}</code></a></li>
        {/each}
      </ul>
      {#if cursor && !issue}
        <button class="app-control wide" type="button" onclick={more} disabled={loading} aria-busy={loading ? 'true' : undefined}>{loading ? 'Loading…' : 'More workspaces'}</button>
      {/if}
    {:else}
      <p class="center" role="status">No workspaces to show for this address. Only workspaces where it is a verified recovery email appear here.</p>
    {/if}
    <nav class="links"><a href="/">Enter a workspace URL</a>{#if issue}<a href="/workspaces">Request a new link</a>{/if}</nav>
  {/snippet}
</Proof>

<style>
  .center {
    text-align: center;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border-soft);
  }
  li a {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    padding: 8px 4px;
    border-bottom: 1px solid var(--border-soft);
    text-decoration: none;
    font: 13px var(--font-sans);
  }
  li a:hover .name {
    text-decoration: underline;
  }
  .name {
    color: var(--declare);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  li code {
    color: var(--muted);
  }
</style>
