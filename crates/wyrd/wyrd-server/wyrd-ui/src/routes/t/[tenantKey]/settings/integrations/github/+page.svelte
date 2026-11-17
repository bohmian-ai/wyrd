<!--
  Settings → Integrations → GitHub: the tenant's one GitHub App connection.
  Connect asks the host for GitHub's URL and continues there in this tab;
  disconnect is one confirmed host request. Each is sent once while pending,
  and the page always shows the connection as the server reads it back.
-->
<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { EmptyState, GithubIntegration, type ActionIntent, type ActionState, type CardRef, type Problem } from 'wyrd-ai-ui';
  import { validateIntent } from 'wyrd-ai-ui/loaders';
  import Crumbs from '$lib/Crumbs.svelte';
  import { repositoryHref, undeclaredHref } from '$lib/repositories-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const here = $derived(`${base}/settings/integrations/github`);
  let actions = $state<Record<string, ActionState>>({});

  /** One host request per press: pending until the server answers, then the connection is read back. */
  async function send(name: 'github.connect' | 'github.disconnect', method: 'POST' | 'DELETE') {
    if (actions[name]?.state === 'pending') return;
    actions[name] = { state: 'pending' };
    const response = await fetch(`${here}/connection`, { method }).catch(() => undefined);
    const body = (await response?.json().catch(() => null)) as { authorization_url?: unknown; code?: unknown } | null;
    // GitHub's own pages finish the connection; the control stays pending while the tab leaves.
    if (response?.ok && name === 'github.connect' && typeof body?.authorization_url === 'string' && /^https?:\/\//.test(body.authorization_url))
      return void window.open(body.authorization_url, '_self');
    await invalidateAll();
    if (response?.ok && name === 'github.disconnect') delete actions[name];
    else if (body && typeof body.code === 'string') actions[name] = { state: response?.status === 401 || response?.status === 403 ? 'denied' : 'error', problem: body as unknown as Problem };
    else actions[name] = { state: 'error', problem: { code: 'WYRD_SPEC_500_INTERNAL', title: 'The request did not reach the server.' } };
  }

  function onaction(intent: ActionIntent) {
    const valid = validateIntent('GithubIntegration', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const input = valid.input as { card_ref?: CardRef; repository_id?: string };
    if (valid.name === 'github.connect') return void send(valid.name, 'POST');
    if (valid.name === 'github.disconnect') return void send(valid.name, 'DELETE');
    if (input.card_ref) return void goto(repositoryHref(base, input.card_ref));
    if (input.repository_id) return void goto(undeclaredHref(base, input.repository_id));
  }
</script>

<svelte:head><title>GitHub · Settings · {data.tenant.name} · Wyrd</title></svelte:head>
<Crumbs items={[{ label: data.tenant.key }, { label: 'Settings', href: `${base}/settings` }, { label: 'Integrations' }, { label: 'GitHub' }]} />
<div class="content">
  {#if 'code' in data.github}
    <EmptyState state="error" problem={data.github} />
  {:else}
    <GithubIntegration {...data.github} {actions} {onaction} />
  {/if}
</div>

<style>
  .content {
    padding: 16px 24px 24px;
  }
  @media (max-width: 767px) {
    .content {
      padding: 16px;
    }
  }
</style>
