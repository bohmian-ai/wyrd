<!--
  Repositories: Repository Cards beside the GitHub repositories the tenant
  can reach. Search and Show live in the URL and travel with every row a
  reader opens, so the way back restores the same rows.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { EmptyState, RepositoryIndex, type ActionIntent, type ActionState, type CardRef } from 'wyrd-ai-ui';
  import { validateIntent, type RepositoryIndexInput } from 'wyrd-ai-ui/loaders';
  import Crumbs from '$lib/Crumbs.svelte';
  import { repositoryHref, undeclaredHref, writeIndex } from '$lib/repositories-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  let actions = $state<Record<string, ActionState>>({});

  async function onaction(intent: ActionIntent) {
    const valid = validateIntent('RepositoryIndex', intent);
    if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
    const back = data.search || undefined;
    actions[valid.name] = { state: 'pending' };
    if (valid.name === 'card.open') await goto(repositoryHref(base, (valid.input as { card_ref: CardRef }).card_ref, { back }));
    else if (valid.name === 'repository.declare') await goto(undeclaredHref(base, (valid.input as { repository_id: string }).repository_id, back));
    else if (valid.name === 'connection.open') await goto(`${base}/settings/integrations/github`);
    else await goto(`${base}/repositories${writeIndex(valid.input as RepositoryIndexInput)}`, { keepFocus: true, noScroll: true });
    delete actions[valid.name];
  }
</script>

<svelte:head><title>Repositories · {data.tenant.name} · Wyrd</title></svelte:head>
<Crumbs items={[{ label: data.tenant.key }, { label: 'Repositories' }]} />
<div class="content">
  {#if 'code' in data.index}
    <EmptyState state="error" problem={data.index} />
  {:else}
    <RepositoryIndex {...data.index} {actions} {onaction} />
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
