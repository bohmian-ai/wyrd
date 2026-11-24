<!--
  Settings → Integrations: the services this workspace reads from. Connect
  asks the GitHub connection endpoint for GitHub's URL and continues there in
  this tab; managing or disconnecting happens on the GitHub page.
-->
<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import type { WyrdProblem } from '$lib/server/problem';
  import Pill from '$lib/Pill.svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const github = $derived(`/t/${encodeURIComponent(data.tenant.key)}/settings/integrations/github`);
  const STATE = { active: ['ok', 'Connected'], suspended: ['warn', 'Suspended'], not_connected: ['off', 'Not connected'] } as const;
  let pending = $state(false);
  let refused = $state<WyrdProblem | null>(null);
  const problem = $derived(refused ?? data.problem);

  /** One request per press; GitHub's own pages finish the connection. */
  async function connect() {
    if (pending) return;
    pending = true;
    const response = await fetch(`${github}/connection`, { method: 'POST' }).catch(() => undefined);
    const body = (await response?.json().catch(() => null)) as ({ authorization_url?: unknown } & Partial<WyrdProblem>) | null;
    if (response?.ok && typeof body?.authorization_url === 'string' && /^https?:\/\//.test(body.authorization_url))
      return void window.open(body.authorization_url, '_self');
    pending = false;
    refused = body?.code ? (body as WyrdProblem) : ({ code: 'WYRD_SPEC_500_INTERNAL', title: 'The request did not reach the server.', remediation: 'Please try again later.' } as WyrdProblem);
    await invalidateAll();
  }
</script>

<svelte:head><title>Integrations · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead"><div><h1>Integrations</h1><p>Services {data.tenant.name} reads from.</p></div></header>

{#if problem}
  <div class="app-problem" role="alert" data-state={problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{problem.status === 403 ? '⊘' : '✕'} {problem.title}</strong>
    <p>{problem.remediation}</p>
    <code>{problem.code}</code>
  </div>
{/if}

<section class="card">
  <div class="row">
    <div class="label"><b>GitHub</b><span>Read repositories and report Verifier results on pull requests.</span></div>
    <div class="ctl">
      {#if data.github}
        {@const [tone, label] = STATE[data.github]}
        <Pill {tone} {label} />
        {#if data.github === 'not_connected'}
          <button class="app-control primary" type="button" disabled={pending} onclick={connect}>{pending ? 'Connecting…' : 'Connect GitHub'}</button>
        {:else}
          <a class="app-control" href={github}>Manage</a>
        {/if}
      {:else}
        <a class="app-control" href={github}>Open</a>
      {/if}
    </div>
  </div>
</section>
