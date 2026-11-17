<!-- A reachable GitHub repository with no Repository Card: how to declare one. Nothing here registers anything. -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { EmptyState, RepositoryDeclare, type ActionIntent, type ActionState } from 'wyrd-ai-ui';
  import { validateIntent } from 'wyrd-ai-ui/loaders';
  import Crumbs from '$lib/Crumbs.svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const name = $derived(!('code' in data.declare) && data.declare.result.state === 'populated' ? data.declare.result.data.fullName : 'GitHub repository');
  let actions = $state<Record<string, ActionState>>({});

  async function onaction(intent: ActionIntent) {
    const valid = validateIntent('RepositoryDeclare', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    await goto(`${base}/settings/integrations/github`);
  }
</script>

<svelte:head><title>{name} · Repositories · Wyrd</title></svelte:head>
<Crumbs items={[{ label: data.tenant.key }, { label: 'Repositories', href: `${base}/repositories${data.back}` }, { label: name }]} />
<div class="content">
  {#if 'code' in data.declare}
    <EmptyState state="error" problem={data.declare} />
  {:else}
    <RepositoryDeclare {...data.declare} {actions} {onaction} />
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
